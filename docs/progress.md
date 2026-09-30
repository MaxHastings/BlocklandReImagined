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

- 2026-09-28 Player names reach the server. In the Internet playtest everyone
  still joined as "Blockhead" after typing a name on Player (Avatar) and
  clicking Done. Cause: `Avatar::read_fields` read each box's label text
  (`View::text_of`), which only holds what the screen last wrote, instead of
  the typed edit value (`View::edit_text`). Done therefore saved and sent the
  old name. The join path, settings file and server were fine. Fixes: Done now
  reads the typed Name, Clan Prefix and Suffix; the "LAN Name:" label reads
  "Name:"; Done while connected sends a new `Command::SetName` (appended last
  in `session::Command`; Gate owns the protocol version) so the server renames
  the player live, updates admin and minigame records, and posts "Old is now
  known as New." v20 only applied the name on the next join. Main's join-time
  "Name 2", "Name 3" numbering now also covers resume and rename.
  The client clamps the hello name to the server's 48-byte rule. A first-open
  prompt built on v20's `regNameGui` window ("Choose Your Name", prefilled
  "Blockhead" plus four digits, OK or Skip) appears in `--run` while the saved
  name is still Blockhead; `$pref::Player::NamePrompted` remembers the answer.
  Evidence: `cargo test -p bri-client --test player_name -- --ignored`
  (drives the real Avatar screen, hosts LAN, joins over loopback, renames
  live, checks the duplicate suffix, and the first-open prompt with a render
  at `artifacts/native-player-name/first-open-name-prompt.png`); new tests in
  `crates/sim/tests/session.rs`, `screens::avatar` and `screens::name`.
  `cargo test --no-fail-fast` for bri-sim, bri-admin, bri-minigames, bri-net,
  bri-ui and bri-client passed, except one bri-net LAN discovery test that lost
  port 28050 to a parallel run and passed on rerun.

- 2026-09-29 Torque ML text for prints, chat and message boxes. Maxwell saw an
  event center print show `<color:FFFFFF>...<br>...` literally in black, and
  Tutorial prompts in inconsistent colours. Two causes: the client escaped
  every server tag except `<bitmap>` (`<` became `‹`), and the old ML layout
  ignored colour, font, shadow, margins and tabs. Separately, Torque's
  `GuiControlProfile` aliases `fontColor/HL/NA/SEL` to `fontColors[0..3]`,
  and v20 assigns `fontColors[n]` last in every conflicting profile (checked
  against all 222 aliased slots in allClientScripts). So the chat/print base
  colour is `fontColors[0]` = 255 0 64, the same colour `\c0` restores; the
  pack kept the overwritten black. `bri-ui::ml` is now the one parser, layout
  and renderer for every `GuiMLTextCtrl` (center/bottom prints, chat, message
  boxes, authored ML controls). It covers `<br>`, `<color>`, `<shadow>`,
  `<shadowcolor>`, `<font>` (nearest cached size of the face), `<just>`,
  `<lmargin[%]>`, `<rmargin[%]>`, `<tab:..>`, `<spush>/<spop>`, links,
  `<linkcolor[hl]>` and `<bitmap>`, plus `\c0-9`, `\cr/\cp/\co`,
  and `allowColorChars`. Unknown well-formed tags are dropped;
  a stray `<` stays text. Untrusted markup is bounded (64 KiB source, 4 KiB
  per message, 1024 tags, depth 32, 64 bitmaps, 512 lines). Bitmaps resolve
  only to pack images under `base/client/ui/` or `add-ons/`. `Pack::from_parts` applies the colour aliasing. Chat lines are wrapped
  in `<spush>/<spop>` as `NewChatSO::addLine` does, and player lines use v20's
  `\c7\c3name\c7\c6: text` format; the old blanket `\c6` prefix is gone. Player
  names and typed chat stay literal. Evidence: `cargo test -p bri-ui` (80 unit +
  integration, 5 ignored real-pack renders pass), `cargo test -p bri-client
  --lib app::tests`, the loopback host+guest test `crates/client/tests/
  ml_text_flow.rs`, and before/after captures from `ml_text_probe` in
  `artifacts/ml-text/{before,after}`. The protocol did not change.
  Follow-ups the same day. Chat links: player messages get v20's server-side
  linkification (mainServer.cs:1136-1166: the first http/https address
  becomes `<a:url>url</a>` without the scheme, then `\c6`), done in the
  client's chat formatting. With the cursor toggled on (M), clicking a chat
  link asks "Open this link in your web browser?" before `OpenUrl`; only
  http/https open, and scheme-less links get `http://` like `gotoWebPage`.
  Wrapped colour: Torque's `drawAtomText` sets the atom's style colour before
  `drawTextN` applies `\cN` codes, and atoms start at every tag, tab, line
  break (`emitTextToken`) and wrap (`splitAtomListEmit`; read in Torque3D's
  guiMLTextCtrl.cpp, same TGE lineage, inferred for v20). So a colour code
  ends at a wrap and the continuation is the base colour; the layout now does
  that. `lineSpacing` is not applied, since that layout never reads it.
  The v20 mouse tip `\c6TIP: Press M to toggle mouse and click on links`
  (MouseToolTip, BlockChatTextProfile, x 2, one 18 px line below the chat
  text) shows while a shown chat line has a link outside single player, or
  while the cursor is toggled on, with `$pref::HUD::showToolTips` on and a
  positive chat line time (c:5370-5392, c:14906-14968, c:15121-15170).

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
- 2026-09-28 Build disk writes. Maxwell asked why builds write hundreds of GB.
  Cause: every dev build carried full debug info, the gate kept incremental
  state and every superseded test binary (953 executables for 149 targets,
  296 GB in `../.bri-gate/target`), and each of ~76 worktrees built from cold.
  - `[profile.dev] debug = "line-tables-only"`, dependencies `debug = false`.
  - The gate builds with `CARGO_INCREMENTAL=0`, drops `debug/incremental`
    before each run and empties its target dir once `debug/deps` passes 40 GB.
  - Worktrees now live under `../BlocklandReImagined-worktrees/` and are
    reused (AGENTS.md). 46 merged worktrees were removed and 323 GB of idle
    build output cleared.
  Evidence: cold `cargo build --workspace --all-targets --locked` of e880717
  in a fresh worktree: 42.1 GB (329 s) as before, 13.5 GB (253 s) with the
  profile change, 8.9 GB (200 s) with incremental off as well.

## 2026-09-28 Friends-ready gaps: version, updates, crash files, signing, low-end PCs

Closes the four gaps between "Max can play" and "friends can play", with no
third-party service and nothing to host.

- Version: `crates/client/build.rs` stamps the build. `BRI_VERSION` names a
  release (the dist folder's version); otherwise the build is
  `dev-<commit date>`. The short hash is always added. The main menu's
  `MM_Version` line, `bri-client --version`, the console `version` command,
  session logs and crash reports show it. `package_playtest.ps1` refuses an
  executable whose `--version` differs from `-Version`.
- Updates: a release build asks
  `api.github.com/repos/MaxHastings/BlocklandReImagined/releases/latest` once
  per start on a background thread (`crates/client/src/updates.rs`, ureq on
  rustls/ring with the platform's certificate store). A release published
  after this build's commit with a different tag shows a yes/no box
  (Open the download page) outside a game, and the version line names it. No
  download; silent offline, on 404 (nothing published yet) and in dev builds.
  Options > Advanced > "Check for new versions" (`$pref::Net::CheckForUpdates`)
  turns it off. Checked from the cloud: the real endpoint answers 404 until a
  release exists.
- Crash files: the next start already showed a dialog with an Open folder
  button (Foundations). It now names the report and, after a native crash,
  the `.dmp`, and says nothing is sent automatically.
- SmartScreen: `-SignCertificateThumbprint` signs and timestamps every `.exe`
  with signtool when Max has a certificate; `PLAYTEST.md` explains More info,
  Run anyway, and Unblock.
- Low-end PCs: on the first run (`App::player_session`, never tests or
  `--check`) the client picks Low (software adapter), Medium (integrated GPU;
  Low above 1080p) or High (graphics card) and saves it as the Options
  quality prefs, with `$pref::Video::AutoQuality` recording the pick; any
  quality pref the player set wins. Values match PR #10's presets so Options
  shows the name. The session log gets a frame-time line each minute of play
  (average fps, median, 1% slowest, worst, frames under 30 fps).
  `PLAYTEST.md` has a slow-PC checklist.

Evidence: `cargo test -p bri-client --lib updates quality`, `cargo test -p
bri-ui --lib gui_options`, `cargo test -p bri-crash --lib`, clippy -D warnings
on the three crates, all content-free. Not measured: how the picks feel on a
real integrated GPU; needs a weaker PC.
## 2026-09-28 Cold walkthrough of the Add-On guide

- Followed `docs/modding/README.md` as a newcomer and made a rule, a HUD, a
  weapon and (by importing a hand-written v20 brick Add-On) a brick pack.
  Stumbles: the README did not link the guide; nothing said where an Add-On
  folder goes; `bri-addon-check` was not mentioned; trying a rule meant
  writing a Rust test; many "coming soon" notes had landed (Add-On weapons
  and bricks in hosted games, HUD drawing, the Import button, game modes);
  command `args` syntax and the hooks' `player` parameter were undocumented;
  no route to new bricks was described.
- Tooling: `bri-addon-check` now shares the Add-Ons screen's side rule
  (`bri_package::library::side_for_kinds`; weapons and bricks are shared,
  `archetype` is a server kind on the screen too) and validates
  `assets/weapons.json`. New `bri-addon-run` (bri-sim) runs an Add-On with
  what it needs, a Host and a Guest, and `--send`/`--wait` commands.
- Not fixed: players cannot type Add-On commands in chat (`/gift 1` is
  "Unknown command"); only HUD keys without arguments reach a script. The
  packaged game ships only `bri-import-addon.exe`, not the check/run tools.
- Evidence: `cargo test -p bri-package -p bri-package-runtime`,
  `-p bri-sim --test addon_run`; clippy `-D warnings` on those targets; every
  command in the guide run as written against copies of the samples.
- 2026-09-28 red team of Add-On client code and hosting
  (`docs/audits/red-team.md`, branch `claude/red-team-o8nvo2`). Fixed with a
  test each: idle or spoofed connections taking every server slot (QUIC
  retry, four unjoined connections per address), Add-On recursion
  overflowing the game's main-thread stack (wasm stack 256 KiB), trust
  grants covering capabilities the prompt never showed, Add-On trust keyed
  by typed address instead of host key, LAN listings overriding saved pins
  (joining and starring), a join hung by a host that allows no streams, and
  shader cost undercounting large values. Open, ranked in the audit: one
  player holding the shared request budget, silent pin replacement after
  "identity changed", PR #1's 4 GB unprompted download, unverified host
  names. Evidence: `cargo test -p bri-client-sandbox`, `-p bri-net`,
  `-p bri-client --lib`, clippy `-D warnings` on those three (Linux).
- 2026-09-28 third-person crosshair (branch `claude/crosshair-v20`). Max saw
  the crosshair centred on his own head in third person (a15). v20's scripts
  never hide it on a camera switch (`toggleFirstPerson` only flips
  $firstPerson, c:20897); the engine's `GuiCrossHairHud::onRender` returns
  unless the control object is a Player or Vehicle and the connection is
  first person (the TGE code Torque3D kept, T3D/fps/guiCrossHairHud.cpp
  116-122). It never projects the crosshair onto the aim point. Our HUD drew
  it every frame. Now the client sends `UiUpdate::FirstPerson` each frame and
  the PlayGui shows `Crosshair` only in first person, which also hides it for
  the dead orbit camera and observers. F5 (`ToggleShapeNameHud`) now hides it
  with player names, as in v20 (c:5890). Evidence: new
  `crosshair_shows_only_in_first_person_and_hides_with_names`,
  `cargo test -p bri-ui`, `cargo check -p bri-client`.

- 2026-09-27 Admin camera (F8/F7) matches v20 (`serverCmdDropCameraAtPlayer`,
  `serverCmdDropPlayerAtCamera`, `cameraImage`, `Player/Vehicle::teleportEffect`
  in the recovered allGameScripts.cs). The server now keeps each connection's
  camera transform, reported with the client's moves. F7 puts the eye at the
  camera unless the ground is closer than eye height (then the feet stand on
  it); without F8 it goes back to where the camera was left; a rider's vehicle
  (jeep, horse) takes the camera transform and stops instead; a dead admin
  respawns at once (new minigame `AdminRespawn`). In a minigame F7 costs a
  point and sets `lastF8Time`: weapons fire and damage nothing for 3 s, and
  with weapon damage on, pickups and activation wait 5 s. Free cameras stream
  as `Orb` datagrams (protocol VERSION 24) so other players see the
  CameraEmitterA glow; the owner does not (`firstPersonParticles = 0`). F7,
  /find, /fetch and /warp play PlayerTeleportEmitterA for 150 ms and a
  player's 3 s PlayerTeleportImage back sparkle (emitters from
  effects-pass-004, no pack change; PlayerTeleportExplosion has no sound).
  Not matched: explosion scale (vehicle bursts are player-sized) because the
  effects runtime has no source scale. Evidence: `cargo test -p bri-sim`
  (session, combat, `vehicles -- --include-ignored admin_drop`),
  `cargo test -p bri-client --test actor_effects`, net codec worst case.
  Needs Max's playtest: F8 orb seen from a second client, F7 on foot, in a
  jeep and on a horse.
- 2026-09-28 night QA map findings (`claude/map-fixes`). The Slopes: planting
  refused any dip into terrain as Buried; terrain contacts are now judged
  only by a whole-footprint "wholly under the surface" test (an
  approximation: v20's engine buried test is unavailable; its script sinks
  terrain ghosts 0.1). The Tutorial always starts single player. Strata's
  guest plant and Slate Sea's hammer were harness aim (Stuck on the host's
  feet; seabed beyond hammer reach); the harness now records plant icons.
  Evidence: `cargo test -p bri-sim --release` (the 22 failures need imported
  content absent from the worktree), night QA matrix `BRI_QA_MAPS=slopes`
  (The Slopes ok; Strata plants, then stops at finding E).
- 2026-09-28 first-impressions follow-up (`claude/first-impressions`): status of
  all 20 items against main in `docs/audits/night-qa.md`. Fixed: duplicate
  player names are numbered, the loading screen names the map, `bri-server`
  takes `resume` for its newest save, and the PLAYTEST/KNOWN-ISSUES drift.
  The remaining items (plant warning, first-run prompts, visible distance,
  accessibility options, brick search and limits, duplicator, input
  toggles, gamepad) follow on the same branch. Evidence: `cargo test
  --release -p bri-sim --test hardening_session`, `-p bri-world --lib
  persistence`, `-p bri-ui`.
- 2026-09-28 baseplate gap on Bedroom's carpet (a15 report). Cause: bricks
  stack on a 0.2 plate lattice from height zero, and map floors are off it
  (Bedroom carpet 286.312, Kitchen spawn floor 119.784, Tutorial 94.406 and
  102.406), so a baseplate rested on the next plane up, 0.088 above the
  carpet. Not an Add-On leak: the lattice is `grid::CELL`, a constant no
  package script can reach. v20 behaves the same: all 13 stock Bedroom saves
  put baseplates at 286.4, and v20's stock layouts sit on the nearest plane
  to each floor (Tutorial bricks dip 0.006, Pirate World 0.034). Fix:
  `Scene::floor_lift` moves the whole map so the interior floor under the
  first spawn lies on the nearest plane (Bedroom +0.088, Kitchen +0.016,
  Tutorial -0.006; terrain maps and the Slate family unmoved). `NativeMap::load`
  and `load_map_bundle` apply the same lift to scene nodes, terrain origins
  and water volumes, so collision and view agree and no brick coordinates
  change. Other surfaces keep their offset: Bedroom's 354.062 furniture top
  goes from a 0.062 dip to 0.15 (Beta City 16, Mansion, Facechild's House),
  Kitchen's main floor stays 0.1 off. Evidence (BRI_CONTENT set, ignored
  tests): `-p bri-sim --test floor_flush_native` (a plate on each of 13 map
  floors flush; The Slopes is terrain), `--test stock_saves_native` (all 35
  reference worlds and both Tutorial layouts load with every brick; nothing
  over Bedroom's carpet hovers), `-p bri-render --test map_floor`,
  `-p bri-content --lib scene`, `-p bri-sim --lib`.
## 2026-09-28 Skis matched to v20

- Audited the skis against v20's `Item_Skis` add-on and the WheeledVehicle
  code in `blocklandv20.exe`; the table is `docs/audits/skis-v20.md`.
- Decoded new engine rules: `isSled` (datablock +0x378) keeps the surface
  forces off unless wheel 0 is on the ground (0x57565f); `onWreck` fires when
  the body collides with none of wheels 0-2 on the ground (0x572303);
  `jumpForce` is not a WheeledVehicle field; VehicleData defaults (impact
  speeds 25/25/50, collision damage 20 and 0.05).
- Fixed: skis now move by Blockland's WheeledVehicle forces (thrust to 40,
  speed-scaled sideways grip on the ground only, squared mouse steering that
  needs speed, the decoded steering return, drag), no invented brake, the
  decoded crash rule, collision-only impact puffs, `Impact1BSound` past 10,
  skis in the skier's last colour can, the two-second "Can't use skis right
  now." centre print, and boarding after 250 ms regardless of reach.
- Adaptations: the skis collide as the box around their hulls, and their
  `bodyFriction` is applied at the centre of mass, because Rapier's contacts
  on the original hull spun sliding skis round.
- Follow-up (protocol 39): tire spray for every wheeled vehicle from the
  decoded `WheeledVehicle::advanceTime` rule, with wheel ground contact now
  replicated; vehicle crash damage removed, because v20 never applies
  `collDamage*` (only networks it), and every vehicle plays its own impact
  sounds. The first version of the audit had the damage backwards.
- Evidence: `cargo test -p bri-vehicles` (new `tests/skis.rs`),
  `cargo test -p bri-weapons -- --ignored`,
  `cargo test -p bri-sim --test vehicles -- --ignored`.
- 2026-09-28 Demo Pong events audit (`docs/audits/pong-events.md`, branch
  `claude/pong-events`). v20's Bedroom "Demo Pong" (277 rows, 4 inputs, 13
  outputs) imported fully but did not play: the served ball was deleted
  inside its serve brick. Fixed: projectile rays ignore the brick they start
  in; `Projectile Explode` responses; projectile responses refresh when rows
  toggle; event delays start at the tick an input fires; counters start
  from an unresolved `Letters/N` print. Evidence: `cargo test -p bri-sim
  --test pong -- --ignored` (rally, points each side, win, reset, paddle
  state machine), `--test events_native -- --ignored`, `cargo test -p
  bri-events -p bri-weapons -p bri-sim`, clippy on those.
- 2026-09-28 floor placement follows v20's nearest plate plane. Max recalled
  v20 never refusing an ordinary floor placement. Ours could: the client lifted
  a brick to the first non-penetrating plane (up to 0.198 above the floor) and
  the authority's support probe only reaches 0.1 below a brick, so floors more
  than half a plate above a plane got a Float refusal. The client now rests
  map-floor placements on the nearest plane, and the authority allows
  `FLOOR_DIP` (0.1) into upward-facing map surfaces only, matching v20's stock
  layouts (Town dips 0.084, a Bedroom shelf 0.062, Pirate World 0.034). This
  supersedes the earlier "never relax collision validation" note for floors;
  walls, ceilings and moving entities keep zero allowance. Evidence:
  `-p bri-client --lib building::`, `-p bri-sim` (all targets, content linked),
  `--test stock_saves_native -- --ignored` now re-plants every stock brick
  resting on a map floor (within half a plate) with no Buried/Float refusal.
- 2026-09-27 Chat colours, emotes and slash commands. The chat HUD drew every
  line from a `\c6` (white) prefix and player lines as plain "name: text".
  Now player chat is v20's `'\c7%1\c3%2\c7%3\c6: %4'` (yellow name, white
  text) and team chat `'\c7%1\c3%2\c7%3\c4: %4'` (the name was grey). Each
  chat line starts in `BlockChatTextProfile`'s base colour, which is Torque's
  `fontColors[0]` "255 0 64" because `fontColor` is that same field and the
  profile assigns it last. The UI importer loses that assignment order, so
  the chat node carries the colour as a runtime tint. Uncoloured server lines
  such as death messages are therefore red-pink, as in v20. Colour codes
  before a death icon were being stripped; they now survive. Added the missing
  lines: `\c2Welcome to Blockland %1.`, `\c1%1 spawned.`, `\c2%1 has become
  Super Admin (Host)`, Admin/Super Admin (Auto), and (Password),
  `\c3%1\c2 failed to guess the admin password.`, kick in the LAN form (no
  BL_ID), ban and permanent ban (with an 8-hex principal prefix standing in
  for the BL_ID), and `\c5Team chat disabled - You are not in a mini-game.`
  Settled the chat line spacing question: `GuiMLTextCtrl` registers
  `lineSpacing` but its layout never reads it. `emitNewLine` advances by the
  font height only (read in Torque3D's guiMLTextCtrl.cpp, which shares the
  TGE lineage; this is inferred for v20, not checked against the
  disassembly). Our layout already matches, so `lineSpacing = 12` stays unused.
  Emotes: v20 has `/alarm /love /hate /confusion /wtf` (Emote_* add-ons) and
  `/bsd /hug /zombie /sit` (base scripts). Added `/bsd` (BSDExplosion at the
  eye point), `/wtf` (= confusion) and `/hug` `/zombie`
  (`playThread(1, armReadyBoth)`, drawn with the existing armReadyBoth clip
  until the held-arm pose changes or the player dies). `Player::emote` spawns
  at `getEyePoint()`, which is m.dts's Eye node at 2.156 above the feet. We
  had used the 2.4 camera-eye assumption, so the alarm "!" sat 0.24 too high.
  Head-slot image emitters ejected along the mount's up axis. v20
  (`ShapeBase::updateImageState`) ejects along the image's +Y column after
  its `rotation`. Love, confusion and pain now spray forward around the face,
  and HateImage's `rotation = "1 0 0 -90"` (already in weapons-pack-008)
  sends its steam upward. Evidence: `cargo test -p bri-sim --test session
  join_admin_team_chat_and_emote_lines_use_v20_colors`, `cargo test -p
  bri-client --lib chat_lines_carry_v20_colors`, `cargo test -p bri-client
  --test actor_effects`.

## 2026-09-28 Brick damage matched to v20

Audited weapon, tool and event brick damage against v20's `onExplode`,
`onCollision`, `miniGameCanDamage`, `fakeKillBrick`, hammer, wand, admin wand
and undo; see [audits/brick-damage.md](audits/brick-damage.md). Our rules
already matched: fake kills with respawn for weapons, deletion for tools,
LAN/single player breaking anyone's bricks outside minigames, internet
servers limited to the shooter's own, and the Brick Damage setting in
minigames. One gap fixed: a rocket in flight still broke bricks after its
shooter's F8 drop in a minigame. v20 ignores brick hits for 3 s after F8, and
`blow_up_bricks` now does too. Evidence: `cargo test -p bri-sim --test
brick_damage -- --include-ignored` (8 tests; the F8 test fails without the
fix). No wire change.
## 2026-09-28 Net graph and performance overlay

The v20 net graph is now its own screen again. Ctrl+N (rebindable as
"Toggle NetGraph") draws `NetGraphGui`'s six 200-sample plots and labels at
their authored places in the converted `NetGraph*Profile` styles. It
replaces the one-line FPS/ping text. The data is QUIC's own counters through
`bri_net::client::LinkProbe`. There is also a new performance overlay: F3
cycles compact, expanded and off, and Ctrl+F3 saves a JSON capture to
`captures/`. It shows FPS, a frame-time graph and the CPU, GPU and wait
split (GPU from timestamps around the frame's encoder). Hosts also see the
server's tick time, per-Add-On script time, world counts and memory. There
is no protocol change, so server figures are host-only. Nothing is drawn or
sampled while hidden. Details, sources and the tradeoffs are in
[audits/net-graph.md](audits/net-graph.md). Evidence: `cargo test -p bri-ui
--test net_graph`, `cargo test -p bri-ui --lib perf`, `cargo test -p
bri-client --lib perf::`, `cargo test -p bri-net --lib perf_window`, and
offscreen renders from the ignored `overlays_render_offscreen`. A live game
window is still unchecked (Max's playtest).
- 2026-09-28 v20 fidelity audit (`docs/audits/v20-fidelity.md`, branch
  `claude/v20-fidelity`). Max reported small integration gaps: a missing sound
  or particle on a weapon or vehicle, feel "not quite right". Three repeatable
  passes. `tools/audit_v20_fields.py` lists every field the 578 stock gameplay
  datablocks set and which source reads it; the 99 unread fields are reviewed
  in the audit. `crates/weapons/tests/v20_fidelity.rs` fires all 21 stock
  items and checks the emitted cues against the literal datablock fields
  (state sounds, emitters, sequences, shells, projectile, damage, impulse,
  explosion effect, sound and radius damage). `crates/client/tests/v20_fidelity.rs`
  checks every cue resolves in the audio and effects packs. Both pass (the
  Horse Ray's scripted `HorseRayProjectile::Damage` is the one exception, as
  in v20). Fixed: explosion debris was never drawn (Jeep tires and wreckage,
  tank turret and hull, cannon barrel, the tank shell's 30 spark streaks),
  now per `Explosion::launchDebris` / `Debris::advanceTime`, lowered from the
  current weapons pack, models from vehicles-pack-011; destroyed vehicles
  played `vehicleExplosionSound` twice; the pirate cannon's "Fire!" power
  bottom print was missing; hard landings did not shake the camera
  (`groundImpactShake*`, inherited TGE code); horses took falling damage
  (HorseArmor `minImpactSpeed` 250), and falls ignored the height scale and
  the admin wand rule. Checked and matching: turret/cannon `activate` (no
  animated nodes in v20), player feel constants, camera, tool slots. Open:
  `jetGroundEmitter` (Blockland-only engine code, needs the disassembly) and
  the bottom print `hideBar` flag (wire field). Evidence: `cargo test -p
  bri-weapons --test v20_fidelity -- --ignored`, `cargo test -p bri-client
  --test v20_fidelity -- --ignored`, `cargo test -p bri-client --lib
  explosion_debris`, `cargo test -p bri-sim --test vehicles -- --ignored
  pirate_cannon`, `cargo test -p bri-sim --test combat horses`,
  `cargo test -p bri-client --test actor_effects hard_landings`.
## 2026-09-28 Loading a save over a build skips overlapping bricks

v20's `ServerLoadSaveFile_Tick` plants each loaded brick and deletes it when
`plant()` fails with an overlap (error 1), stuck (3) or buried (5); the end
line counts them as not created ("5 / 10 bricks created in ..."). Our load
now skips loaded bricks that overlap a brick already in the world or earlier
in the same save, using the same cell rule as planting (`overlaps_world` in
`simulation.rs`), and the existing end line reports them the same way.
Stuck and buried skips are not copied.

That check first skipped 43 bricks, all ramp pairs, in five stock saves
(Arch of Constantine, Jetpuff's Towers, Afghanistan DM, Ice Palace, Sirrus
Military Compound). The cause was `grid::Bounds::cell` reading a BLB's depth
slices back to front: the first slice is the BLB's largest y, which the
converter maps to our smallest z. Symmetric bricks were unaffected. Ramps,
corners and crests had their empty wedge cells and stud ends at the wrong
end for overlap and support, so hand planting also refused ramp arrangements
v20 allows. With the fix, all 37 stock saves load with nothing skipped. Evidence:
`cargo test -p bri-sim --lib grid` (the slice order against `1x3ramp.blb`'s
top quad, and two of the stock ramp pairs), `cargo test -p bri-sim --test
session loading_over_a_build` (two overlapping saves), `cargo test -p bri-sim
-- --include-ignored` (the stock-save test asserts no brick is skipped),
`cargo test -p bri-net --test loopback` (its reload test now loads beside
the first copy). No wire change.

## 2026-09-28 Join trust question for Add-On code

- Tidying the open pull requests (#5, #7-#14) found every one already on
  main through the gate; the one piece PR #11 left unbuilt was the question
  a joining player answers before a server's Add-On code runs, so remote
  code never ran. Now, entering someone else's game whose sandboxed Add-On
  code is not trusted for that host key asks "<server> wants to run Add-On
  code" with each Add-On's plain-words capabilities, "Trust and join" and
  "Leave" (`ClientCode::trust_prompt` / `accept_trust`, `trust_question` in
  `app.rs`). Trust and join saves the grant to `addon-trust.json` and starts
  the code; Leave disconnects. Elevated code is not offered (nothing
  elevated runs yet) and is named in chat instead. The Add-Ons screen gained
  Forget Trust, which clears every grant, so the prompt's "take this back on
  the Add-Ons screen" holds. Message boxes widen a button whose label is
  longer than v20's Yes/No. Asking before the code downloads, and a
  per-server list, remain (`docs/architecture/client-sandbox.md`). Evidence:
  `cargo test -p bri-client --lib client_code`, `cargo test -p bri-ui --test
  runtime_input`.

- 2026-09-28 v20 fidelity, a16 follow-ups (branch `claude/v20-fidelity`, rows
  18 to 23 of `docs/audits/v20-fidelity.md`). Skis were held sideways: v20's
  `eulerToMatrix` goes through `MatrixCreateFromEuler`/`QuatF(EulerF)`, the
  transpose of the `MatrixF(EulerF)` the packs assume (read in OpenMBG's TGE
  sources), so every signed or two-axis `eulerToMatrix` image was mirrored
  (`bri_weapons::rotation`, applied at client load). Deploying a ghost brick
  now fires the grey brick image (Fire swing, `brickTrailEmitter`, blue
  `brickDeployExplosion`). Held hammer/wand/sword/broom swing again in first
  person on every Fire entry. The a16 multiplayer crash ("invalid debris
  collision result") was a gun casing ray starting inside a brick; casings
  now drop, and every per-frame presentation subsystem absorbs its errors
  (`CosmeticFaults`) instead of reaching the fatal dialog. Brick break plays
  once per blast, not per brick. The camera no longer snaps roll at straight
  up/down (look-at with a switched up vector); first person clamps at v20's
  exact +-90 degrees and the chase camera's tilt passes vertical. Evidence:
  `cargo test -p bri-weapons --lib rotation`, `cargo test -p bri-client --lib
  looking_straight`, `cargo test -p bri-client --lib building`, `cargo test
  -p bri-client --test world_items a_held_hammer -- --ignored`, `cargo test
  -p bri-client --lib weapon_debris -- --include-ignored`, `cargo test -p
  bri-client --lib audio -- --include-ignored`, `cargo test -p bri-sim --test
  session deploying_a_brick -- --ignored`.
- 2026-09-28 What other players see (branch `claude/remote-anim`). Max
  reported three multiplayer gaps. (1) Another player's wrench swing went
  much too far. Cause: held images were drawn at their highest detail,
  `detail9999`, the mesh only the holder's first-person view reaches in v20.
  The wrench, hammer, sword and other tools' `fire` sequences animate only
  that mesh (`Wrench9999`, `FPhammer9999`), so others saw the item's
  first-person swing on top of the arm's `wrench`/`armattack` thread. Other
  players' images (and the holder's own third-person view) now use the
  largest detail below 9999 (`detail32`/`detail100`), which the swing leaves
  still, as in v20; the local first-person view is unchanged. The arm
  threads themselves already matched: a new two-client loopback
  (`crates/client/tests/remote_poses.rs`) swings the hammer and wrench from
  each side while looking down and pins the watcher's hand pose to the
  swinger's own (measured equal to 0.001 rad). (2) A rider's look did not reach
  others: the server consumed a mounted player's moves for the vehicle but
  never applied their pitch or free-look head turn, so remote riders sat
  frozen. `Player::look` now applies them each mounted tick (v20
  `updateMove` still turns `mHead` while mounted), for every mount.
  (3) Other players' ghost bricks were invisible (v20 ghosted `tempBrick` to
  everyone). The client reports its ghost (`Command::GhostBrick`, at most
  10 per second, removal at once, with its own 30/s budget outside the 60/s
  action budget); the server keeps it only while its owner lives with bricks
  in hand and replicates it in `Vitals::ghost`; clients draw others' ghosts
  translucent in their colour and shape. Protocol 40 -> 41. Evidence:
  `cargo test -p bri-client --lib -- --ignored others_see_held_tools`,
  `cargo test -p bri-net --test loopback ghost_bricks look_pitch`,
  `cargo test -p bri-sim --test vehicles -- --ignored riders_keep`,
  `cargo test -p bri-client --test remote_poses --release -- --ignored`.
- 2026-09-28 Duplicator (first impressions 18, branch `claude/duplicator`).
  Built as two Add-Ons on new engine seams, modelled on the v20 Duplicator
  Add-On. `packages/duplicator/duplicator` (host rule): `/dup` or
  `/duplicator` gives the Duplicator; its swing copies the clicked brick and
  every brick joined to it through studs that the player may build on,
  never below the clicked brick (so a click on a build's base takes the
  build, not the baseplate it stands on), up to 2000 bricks.
  `packages/duplicator/duplicator-tool` (shared): the tool, the stock wand
  shape coloured blue. Engine mechanisms, with no Duplicator policy in
  them: `bri_sim::blueprint` (a copy held about a stud-corner pivot at its
  bottom, so quarter turns keep it on the grid), `Simulation::build_from`
  and `plant_group` (every plant rule per brick, the world holding up at
  least one, all or none), `Command::PlaceBlueprint` (minigame build rule,
  brick limit, plant rate: a copy needs room in the plant window and uses
  the rest of it, reach), one undo entry per placed copy, script operations
  `copy_build` (new capability `build`) and `give_item` (`player`), and
  Add-On tools: a weapon image with `command` runs that Add-On command on
  `onFire`. The client draws the copy as one translucent ghost while the
  tool is in hand; the numpad moves (super shift by the copy's size) and
  turns it, Numpad Enter places it, Numpad 0 puts it away; the ghost turns
  red when it would overlap or float. Protocol 41 (`Notice::Blueprint`,
  `Command::PlaceBlueprint`, `Image::command`). Custom games on base maps
  now run enabled Add-Ons that need no package world and that no game mode
  claims (`Catalog::for_base_map`), as v20 ran enabled Add-Ons everywhere;
  the Stress Lab's are all claimed by its mode, so Slate stays plain for
  it. Playtest builds ship the Duplicator on (`content/addons/`). Copies
  keep shape, colour, print and FX; names, events, lights, emitters, items,
  sounds and vehicles stay with the original. Evidence: `cargo test -p
  bri-sim --test duplicator` (selection and connectivity, turning on the
  grid, all-or-none planting, trust refusal, one undo, brick limit, plant
  rate and reach, `/dup` and a real swing through the weapon state
  machine, Custom runs it), `cargo test -p bri-sim --lib blueprint`,
  `cargo test -p bri-client --lib a_copied_build`. Not seen in a window:
  Max's playtest.

## 2026-09-28 Network bandwidth audit

Max's rule: never spend bandwidth on things that are cosmetic and heavy.
`docs/audits/network-bandwidth.md` has the inventory, the before and after
table and the follow-ups. Measured over real QUIC on loopback with the host's
new per-kind counters (`ServerHandle::traffic`): eight idle players cost the
host 653 KB/s of upload and each player 86 KB/s, because every pose went to
every player 40 times a second with its field names and a packet of its own.
Now: state datagrams are compact and packed; other players' poses are a
smaller `RemotePose` with quantized velocity and look; still players, parked
vehicles and camera orbs settle then go out once a second; empty updates run
at 10 Hz with absent fields left out; vitals, inventories, avatars and held
images go only for the players that changed; projectiles are sent when they
appear or leave their flight and every client coasts them with
`bri_weapons::coast`; moving Add-On entities send only where they are; a
client drawing above 60 fps no longer sends twice the input. Eight idle
players: 653 to 14 KB/s from the host. Eight running: 657 to 137. A rocket
fight: 849 to 46. Each player's upload: 33 to 13 KB/s. Protocol 46. Evidence:
`cargo test -p bri-net --test bandwidth` (idle, explosion and rocket-fight
byte budgets), `-- --ignored --nocapture` for the table, `cargo test -p
bri-net --lib stream` (settle and keepalive, coasted projectiles match the
host's flight, entity moves, held input merging).
- 2026-09-28 Smooth replicated motion (branch `claude/smoothing`). Max saw
  the football and soccer ball move at the server's update rate. Audit of
  what the client drew between host updates: the local player is predicted
  and remote players interpolate (`motion.rs`); vehicles interpolate, but the
  driven vehicle popped on each 40 Hz pose; projectiles (thrown and kicked
  balls included), dropped items and package entities were drawn straight
  from the 20 Hz reliable deltas, stepping every 50 ms; brick, explosion and
  shell debris are client-side cosmetics advanced every frame already. New
  `crates/client/src/ghosts.rs` does what Torque's ghosts did: projectiles and
  drops fly on the client from their newest update with the host's tick
  integration, gravity (`gravityMod`, Item gravity 20) and Torque's
  `Projectile` bounce (reflect, friction, elasticity) against the client's
  map and bricks; package entities interpolate a jitter-sized delay behind.
  A disagreeing update keeps the drawn pose where it was and decays the
  difference (Torque's warp); the stream clock slews rather than jumps.
  Projectiles fly their whole lifetime from one update, so the host may send
  a projectile once plus its corrections (for the bandwidth lane). The
  driven vehicle now warps onto corrected poses too. No protocol change.
  Evidence: `cargo test -p bri-client --test ghost_smoothing` samples every
  144 Hz frame of a bouncing ball and a sliding entity on loopback, with
  80 ms latency plus 60 ms jitter, and with spawn-and-impact updates only:
  worst frame step at most 1.41x the true motion, no stalled frames, worst
  error 0.38 units at bounces; the same measure on raw snapshots shows an
  8.45x step and 435 of 506 frames stalled. `cargo test -p bri-client --lib
  -- ghosts vehicles`. Remote players' clock (`Motion::observe_clock`) still
  jumps forward on an early pose; that belongs to the remote-animation lane.
- 2026-09-28 Pose clock slewing (branch `claude/smoothing`). `Motion`'s
  server clock snapped forward on every earlier-than-ever pose, hitching
  remote players and non-driven vehicles under jitter. It now slews at most
  5% toward its estimate and snaps only past 60 ticks, like `ghosts::Clock`.
  No protocol change. Evidence: `cargo test -p bri-client --lib
  server_clock_runs_smoothly_under_jitter` (80 ms + 60 ms jitter, 144 Hz
  frames): rate within 5%; the snapping clock measured 119% off.

## 2026-09-28 Final touches checked against v20's own files

Max asked why final touches keep slipping through. The cause was that our
tests checked the code against itself, not against v20. This sweep checks
against v20's datablocks, scripts and `blocklandv20.exe`. It records the
evidence in `docs/audits/v20-unread-fields.md` and
`docs/audits/v20-client-scripts.md`.

- The jet ground dust within 4 units of the ground (exe 0x5ad1b0) and a
  duplicated tire-spray update: de7b9c4.
- Name tags are white or the mini-game colour, with an 8-way outline, as
  `GuiShapeNameHud::drawName` draws them. Raycasting bricks hide them
  (mask 0x200001d), and they fade from the fog distance: 24b32b1.
- Opening the brick selector sends the "Bricks" BSD emote everyone sees:
  862b2da. With building disabled, it only prints so. Emotes from a dead
  player pass quietly: 00a658e3.
- All 54 stock item poses and weapon state machines match v20. See
  `tools/audit_v20_poses.py` and `tests/v20_poses.rs`: a46840d.
- A headless sweep of every dialog at 1999x800, 1280x720 and 2560x1440
  (`crates/ui/tests/screen_sweep.rs`). Fixed: Change Map and Server
  Options cut off at 1440p, the Admin Login stray button, the Mini-Games
  status row and v20's never-shown Create blocker: d2c53d48.
- AutoLight on dark maps at each spawn, and a respawned body starts dark:
  2cf3769f. No favorites auto-buy in a local Tutorial: 00a658e3.
- Vehicle steering behind `$pref::Input::UseStrafeSteering` and
  `UseAutoReturnSteering`, recovered from the exe. Protocol 43 adds
  `Command::SteeringPrefs`: ebffe9ce and 85055680.

Evidence:

- `cargo test -p bri-client --test actor_effects --test v20_client_scripts`
- `cargo test -p bri-client --test v20_poses -- --ignored`
- `cargo test -p bri-vehicles --test steering_prefs -- --ignored`
- `cargo test -p bri-ui --test screen_sweep -- --ignored`
- `cargo test -p bri-ui`
- `cargo test -p bri-sim`

Max's playtest will be the first time these are seen in a window.

## 2026-09-28 Pushable knocked-out bricks (client-only, no network)

Max asked for bricks that became rigid bodies (hammer, wands,
`fakeKillBrick`, brick explosions) to be pushed by players, vehicles and so
on, "like v20 a bit more modern", then ruled that this motion is cosmetic like
particles and must never be synced: "dont wanna waste bandwidth" and keep the
network light for things that matter. A first server-simulated version
(0d3fe4a, b5bc0dc, protocol 41) was withdrawn on this branch; the protocol,
server and replication are unchanged from 8874297.

Each client's `BrickDebris` (`crates/client/src/brick_debris.rs`) now takes
the players and vehicles it draws as kinematic boxes (`BrickDebris::push`,
nearest 32 within 6 units of debris), projectiles it draws (`shots`: each
pushes a body it passes once, 0.2 x speed, and flies on), and later blast cues
(radius > 0.5 shoves debris already lying around). Pushers are kinematic, so
debris can never move, slow or block a player or vehicle; nothing about debris
reaches the server, events or any gameplay query. Debris now weighs 5 per
cubic unit (2x4 brick = 6) and stays solid 3 s, fading over 2 s (was 1.5 +
2.5). Evidence: `cargo test -p bri-client --lib brick_debris` (walk-in shove
with the player exactly where the game put it every frame, vehicle ram
scatters 7+ of 9, resting pile stays within 0.05, shots and blasts, 128 bodies
plus a 64-player crowd capped at 32 pushers: ~1 ms/frame in debug).
## 2026-09-28 Pong colours come back

- Using Demo Pong's paddle buttons could leave bricks on the wrong colour.
  Two clicks in one server tick got the same timestamp, and their relays
  reached the paddle state machine together. Both relays used the rows as
  they were before either switched them. Round-robin turns between origins
  could also run a later click's row before an earlier one's. The event
  engine now runs every job from one time-ordered queue, as v20 does. Each
  activation is its own instant inside the tick, and everything it
  schedules inherits that instant. A `cancelEvents` only reaches rows
  scheduled by its own instant. Details and tests are in
  `docs/audits/pong-events.md` ("Colours that stayed changed"). Evidence:
  `cargo test -p bri-events`, `cargo test -p bri-sim --test pong --
  --ignored` (the new hammer test fails on the old scheduler),
  `cargo test -p bri-sim --test events_native -- --ignored`. The event
  checkpoint gained defaulted fields. No wire change.
- 2026-09-28 Remote swing test flake (branch `claude/remote-poses-flake`).
  `remote_poses` compared the peak hand turn each client sampled over
  wall-clock frames; under parallel load frames were far apart and the two
  sides caught different points near the peak (1.018 vs 1.058 rad, limit
  0.05). Frames now advance at most 4 ms of game time and the windows count
  game time, so sampling stays dense however slow the machine is; checks
  unchanged. Evidence: 12 runs six at a time pass, worst gap 0.006 rad; six
  more with the `claude/smoothing` pose clock applied pass.

## 2026-09-28 Paint colour changes fade like v20

- Maxwell remembered undo and painting fading bricks to their new colour.
  v20's scripts only call `setColor`. The engine eases every drawn brick
  colour toward its new colour at `k = 4 * dt` per frame (read from
  `blocklandv20.exe` 0x53cc90), so paint cans, undo, the wrench and
  `setColor` events all fade. The client now does the same
  (`crates/client/src/brick_fade.rs`). Changing bricks leave their chunk and
  are drawn alone until they settle. The recovered rules and limits are in
  `docs/audits/bricks.md` ("Paint colour changes ease in"). Evidence:
  `cargo test -p bri-client --lib`, `cargo clippy -p bri-client --lib
  --tests`, `cargo build -p bri-client --bins`. Not yet seen in a visible
  window. Maxwell's check is to paint a brick, undo it, and watch both fade
  over about a second.

## 2026-09-28 Crash hunt: fuzzing and chaos soaks

Max asked how to find crashes like the a16 casing one before players do.
New test crate `crates/chaos` (see `docs/crash-hunt.md`): bots doing
everything at once in one session (`session_chaos`, deterministic per seed)
and over real QUIC connections (`net_chaos`), plus property tests for Rapier
update patterns, ray and sweep queries, wire decoding, damaged saves, the
weapons runtime and damaged Add-On imports. Content variants are
`#[ignore]`d behind `BRI_CONTENT` for long soaks on Max's PC.
Found and fixed at the root:
- Rapier island panics (debug) and a release out-of-bounds in its sleep scan
  from collision-only refreshes after vehicle spawns and build loads.
  `bri_physics::detect_collisions` now runs a 1e-6 s full step holding
  kinematic targets; every runtime call site uses it (the player-only requeue
  workaround is gone). Regression inputs in `physics_fuzz.rs`.
- Zero normals from rays starting inside bricks in `Simulation::target`,
  `WeaponQuery::sweep`/`sweep_box`, tool melee and client building targets:
  `bri_sim::simulation::hit_normal`.
- `Replica::pose` accepted NaN energy, scale, head turn and jump normal.
- Every spawn point built over refused all joins and failed map changes;
  joins now fall back to a clear point, then place the body anyway
  (`Player::spawn_overlapping`), as v20 and our respawns do.
- `ServerReport` now counts contained step errors (`step_errors`).
Evidence: `cargo test -p bri-chaos` green; `cargo test --no-fail-fast -p
bri-physics -p bri-motor -p bri-sim -p bri-vehicles -p bri-weapons -p
bri-world -p bri-net` green apart from tests that need generated content
(not present in the cloud checkout); clippy -D warnings on every changed
crate. Not covered: the client renderer and audio (need a window), fx-runtime
particles beyond the debris/weapon paths, and `.bls` text import.


## 2026-09-28 Tester guide and feature list

- Outside testers asked for a feature list: what is done and what still
  needs doing. `docs/TESTER-GUIDE.md` (install, first start, LAN and
  internet play by direct IP, where logs and crash files go, what to send,
  known limits) and `docs/FEATURES.md` (v20 features done, partly done and
  missing; what goes beyond v20; Add-Ons; what importing a v20 Add-On
  brings and leaves out) now ship at the top of every release folder
  (`package_playtest.ps1`, `package_playtest.sh`, the packaging test and
  `playtest-package-layout.md`). Every claim was checked against the code
  or the audits (`v20-parity.md`, `v20-fidelity.md`,
  `v20-client-scripts.md`, `spike-addon-import.md`). Only UDP 28000 needs
  forwarding for internet play: 28050 answers LAN discovery and is never
  needed to join (`docs/architecture/hosting.md`). The modding guide no
  longer lists the join trust prompt as unbuilt; only elevated client code
  is. Evidence: `bash -n tools/package_playtest.sh`; the PowerShell
  packaging test needs Windows (not run in the cloud).


## 2026-09-28 Water audit against v20

- Max's Slate Sea screenshot: sand showing over the sea in a raised, notched
  band, and bright turquoise water in visible square tiles. Slate Sea's sea
  sits 9 units over the slate on an opaque sand WaterBlock; our water strips
  sorted by centre, so sand strips drew over the sea. Water now sorts as
  planes, as Torque's WaterBlocks do.
- From `blocklandv20.exe` (details in `docs/audits/water.md`): with no
  terrain the depth masks keep GBitmap's 0xFF fill, so the Slate maps draw an
  opaque shore pass; the depth-mapped path adds the authored specular
  highlight that makes Sea pale; the plain path (Storm, Tutorial) texgens at
  TessSurface/48 per unit without distortion. Texture coordinates continue
  across repeated copies.
- The glitchy rise to the surface: full coverage rounded to 0.99999994 on
  alternate ticks, flipping the forced underwater crouch every tick.
  `Water::coverage` now returns exactly 1 for a body wholly under.
- Dropped items float (every stock ItemData: density 0.2, no drag); vehicle
  water drag is `drag x viscosity x coverage` unscaled by mass, and wheeled
  vehicles' spin decays at that rate, as `WheeledVehicle::updateForces`.
- Split out: the live Sun direction uses the stale `direction` field instead
  of azimuth/elevation (all maps' shading, and the water highlight).
- Evidence: `cargo test -p bri-render -p bri-content -p bri-sim -p
  bri-weapons -p bri-vehicles -p bri-client`; offscreen `scene_snapshot`
  renders of Sea, Storm, Desert and Slopes compared with the map previews.
  No wire protocol change. Needs a visible check by Max (see the thread).
## 2026-09-28 Destructo Wand sound and admin menu (a17 report)

- Max heard the Destructo Wand play a loud brick-break sound where the
  hammer's break is quiet. v20's scripts give both tools the same path:
  the hammer plays `hammerHitSound`, the wand its explosion's
  `wandHitSound` (both `AudioClosest3d`, 5/30), and each `killBrick`
  plays `BrickBreak` (`AudioClientClose3d`, 10/60) on the client. Our
  server cues already match (new `tools.rs` test pins both). The
  difference was the chain kill, which only the wand reaches (the hammer
  refuses bricks that would strand others): we played one break sound
  per popped brick, since each has its own origin. `blocklandv20.exe`
  schedules the client's `BrickBreakSoundEvent` only when a ghost's death
  is at least 80 ms from the last one scheduled for any brick
  (0x539c10-0x539c57, last time at 0x81ac44) and plays it at that brick
  (0x53a130, culled past `maxDistance`). `ClientAudio` now does the same.
  `wandHitSound` stays: it is v20's own wand sound.
- v20's `AdminGui_Wand` pops adminGui and escapeMenu after asking for the
  wand; our admin screen now does too, as Max asked.
Evidence: `cargo test -p bri-sim --test tools`, `cargo test -p bri-ui
--test admin_screens` (the new test fails without the fix), `cargo test
-p bri-client --lib audio -- --include-ignored`, clippy clean on the three
crates. Not run: the gate; Max's interactive check.

## 2026-09-28 Admin orb flies at v20 speeds

- Max: holding left click in the v20 free camera flies faster; ours did not.
  v20's scripts set `$Camera::movementSpeed = 40`; its fly-mode tick
  (blocklandv20.exe 0x588514) doubles that while trigger 0 (left click) is
  held, else halves it for trigger 1 (`altTrigger`, unbound in v20), else
  quarters it for trigger 3 (crouch, left shift). Walk (`c`) scales each
  axis by 0.4, the axes are not normalized (diagonals are faster), and v20
  binds no `moveup`/`movedown`, so space does nothing and shift slows
  instead of descending. Right click is jet, which the camera ignores.
- Ours flew at 30 (8 walking), used space/shift to climb and sink, and
  swallowed left click on the camera. `Controls::fly_speed` now follows the
  exe; left click is recorded while a camera has control and forgotten
  when control returns to the body.
Evidence: `cargo test -p bri-client --lib controls`
(`free_camera_flies_at_v20_speeds`, `leaving_the_camera_forgets_a_held_fire`).
Needs Max's playtest: F8, fly with and without left click and shift.
## 2026-09-28 v20 parity: last missing rows built

- Branch `claude/v20-parity`. Every Part 1 row of `docs/audits/v20-parity.md`
  that was missing is now built (100 present, 5 partial, 0 missing, 7 other
  lanes, 8 dropped of 120). This round: v20's schedule, light and emitter,
  item and projectile quotas per builder (`fbc263ef`; "Too many events at
  once!" as `ProcessInputEvent`); Load Bricks' colour warning with Nearest
  Match and Add More Colors (`c3ad84a0`); Random Brick Color's next colour
  on the ghost and `/clearinventory` (`c72cc900`); a joiner's wrench lists
  only the host's Music Files (`2ce1ecd2`).
- Dropped, each with a v20 reason in its audit row: Replace Current Color
  Set (v20 needed it for a 64-colour set; ours holds 256 and the replicated
  palette only grows), Render My Player, and the Misc quota (explosions are
  instantaneous here).
- Protocol additions for Gate to number: `Notice::TempBrickColor(u8)` and
  `Notice::MusicTracks(BTreeSet<String>)`.
- Evidence: `cargo test -p bri-sim -p bri-events -p bri-client -p bri-net
  -p bri-ui`; `cargo test --release -p bri-sim --test pong --test
  events_native --test special_bricks --test stock_saves_native --
  --ignored`; `cargo test --release -p bri-client --test app_flow --test
  multiplayer -- --include-ignored`; clippy clean on the touched crates.
  None seen in a visible window yet. Maxwell's checks: load a save made
  with other colours (the Color Warning should ask); host with Random Brick
  Color on and watch the ghost change colour after each plant.
## 2026-09-28 Native ports of v20 Add-On scripts

- Imported Add-Ons keep what their scripts did as `needs_behaviour`. There is
  now a list of native ports, `crates/addon-import/ports/ports.json`: one entry
  per v20 Add-On (folder name, checked source hashes, `verified` or
  `partial`, the functions covered and the tests that prove it). A port is
  JSON merge patches for the files the importer writes, plus files it adds.
  It carries only the rewrite, never the original Add-On's files, so the list
  is built into `bri-import-addon` and the in-game Import applies it with
  nothing downloaded. A port applies when the Add-On's name matches and every
  covered function matches the port's patterns; the patterns also read the
  numbers the port uses from that copy's script. A copy that does not match
  is left as imported, and the report names the port and what did not match.
  The report (schema 2) gains `ports`, `port` on each covered entry and
  `needs_behaviour_ported`. IMPORT-REPORT.md marks ported functions and
  points at the recipe for the rest.
- Engine mechanism: an image may carry `shot` (projectiles per shot, v20
  `%spread`, recoil). It is v20's widespread spread `onFire`: recoil first,
  then each projectile's velocity turned by random Euler angles of up to
  ±5π·spread about each axis. The random angles are a hash of the tick,
  shooter and pellet number, so host and players agree with nothing on the
  wire. Recoil is a new weapons event, `Recoil`, that the session applies to
  the shooter's velocity. No protocol change; packs without `shot` serialize
  as before.
- First port: `Weapon_Shotgun` (Sawn-off Shotgun) now fires its pellets with
  spread and recoil instead of one pellet. Recipe for people and agents:
  `docs/modding/porting.md`.
- Evidence (cloud, no archive): `cargo test -p bri-addon-import` with the new
  `tests/ports.rs` (a CC0 stand-in with the shotgun's folder name and
  onFire shape: 5 pellets inside the v20 spread cone, each inheriting the
  recoil; a mismatching copy left at one pellet; a port adding files and
  patching `package.json`; a hosted `Session` where the recoil moves the
  shooter). Still owed on Maxwell's PC: `real_community_samples` and
  `community_shotgun_and_car_work_in_a_hosted_game` now expect the real
  shotgun to be ported (three pellets); they print its sha256 for the list.
- Porting in two commands, from the release folder: `bri-import-addon port
  ADDON DIR` sets up a work folder (plain import and report, the original
  scripts to read, a drafted `port/`, `port/checks.json` stating what v20
  does, `entry.json`, `stubs.rhai` quoting each function still to port, and
  an `AGENT.md` with a prompt filled in for the Add-On). v20's spread-code
  weapons are drafted completely. `bri-import-addon check-port DIR` imports
  again with the port, fires each checked weapon, lists what is still
  unported and prints the ports-list entry (`verified` or `partial`, with the
  copy's hash) to `submit.json`. Evidence: `tests/ports.rs`
  (`port_command_drafts_a_spread_weapon_and_check_port_verifies_it`,
  `hand_ports_start_from_stubs_and_check_as_partial`,
  `port_and_check_port_run_from_the_executable`). The shotgun's own
  `checks.json` (3 pellets) runs in `real_community_samples` on Maxwell's PC.
- Fix: a port that patched `weapons.json` left the item presentation pinning
  the old bytes, so hosts refused the Add-On ("item physics does not match
  its weapons pack"; Gate's hosted shotgun test). Ports now re-pin
  `presentation.json` to the patched `weapons.json` and `item-physics.json`.
  `check-port` checks the pins the way hosts and players do, and the port
  tests assert them.
## 2026-09-28 Add-On presentation never blocks a join

- Max could not join a server running the Duplicator: the join reloaded
  content, the item HUD found 22 weapons against 21 presented items and
  failed with "Item HUD catalog coverage mismatch", and the player saw only
  "Loading the server's Add-Ons". c5115c1 fixed the Duplicator; this makes
  the rule general. `ItemUi::new` builds a row per weapon from the weapon
  list (letter icon when there is no art). `ItemAssets::load_with` presents
  every Add-On item, image and projectile: its own presentation if it
  loads, stock art it names, else no model; broken Add-On textures and
  models become stand-ins listed in `ItemAssets::faults`. The same fallback
  now covers Add-On vehicle models and textures, explosion shapes, death
  icons and brick icons. The base game's packs stay strict so `--check` and
  the gate still catch importer regressions. A join that still fails to load
  the server's Add-Ons shows the player the Add-On and file.
  `bri-addon-check` warns when an Add-On's item presentation is missing or
  stale.
- Tests: `crates/client/tests/add_on_fallbacks.rs` (content-free) hosts four
  broken Add-Ons over loopback, a clean client downloads them and loads the
  item art and HUD; `add_on_join.rs`
  `a_guest_joins_a_host_running_every_repository_add_on` (needs content,
  runs in the gate) hosts every Add-On under `packages/` and joins with a
  base-only guest. Evidence: `cargo test -p bri-client --lib --test
  add_on_fallbacks`, `-p bri-package -p bri-package-runtime`, `-p bri-net
  --lib --test package_sync` green; clippy -D warnings on those crates. The
  content test was not run in the cloud (no generated content).
- Seen once in four runs, unrelated: `package_sync`
  `downloads_reach_only_offered_files` failed its "does not offer package
  downloads" assertion (line 158); it passed alone and three times in full.
- Max, 16:07Z: "should never have an issue joining a server like this ...
  just download whatever we need and play". A join now downloads every
  Add-On the server runs that the joiner lacks or has in another version
  (matched by content hash; a cached copy of another version is never used)
  with no question, however large: the 200 MB download prompt is gone
  (`ASK_ABOVE_BYTES`, `NeedsApproval`, `DownloadDeclined`,
  `UiAction::ApproveDownload`). The 4 GB safety cap stays. Shared Add-Ons
  only the joiner runs no longer refuse the join: `connect_fetching` leaves
  them out and returns them, and `mods::load_fetched` / `joined_set` load
  the server's exact set. The only question a join can ask is the trust
  prompt for sandboxed Add-On code.
- The `downloads_reach_only_offered_files` failure was real: a server that
  refused a connection (no downloads, wrong version, bad identity, full)
  closed it as soon as the refusal was acknowledged, and QUIC discards
  stream data the peer has not read yet, so the player could see
  "connection lost: closed by peer" instead of the reason. The server now
  lingers until the peer closes (`server::linger`, 3 s cap). Before: 4
  failures in 40 full runs of `package_sync`; after: 0 in 160.
- Tests: `package_sync` `a_join_downloads_exactly_the_servers_add_ons_without_asking`
  (a joiner with no Add-Ons and one with a stale copy both run the server's
  exact set) and the E31 test (an extra shared Add-On sits out);
  `add_on_fallbacks.rs` adds a player with a stale copy and an Add-On of
  their own, through `mods::load_fetched`, `joined_set` and the item art.
- Gate's content test `a_guest_joins_a_host_running_every_repository_add_on`
  failed with an empty download. Cause: `App::host()` re-read packages.json
  unless `enable_packages` had chosen the set, so the set the test applied
  with `apply_packages` was thrown away. `apply_packages` now marks the set
  as the player's own; only a join that loaded another server's Add-Ons
  clears that, so the next hosted game re-reads the player's list.
- The same run showed the repository's own Add-Ons clashing: the Stress Lab
  HUD and the Survival Points HUD both used J. The sample Leaderboard key is
  now N (v20 keys untouched; Stress Lab keeps J). Left-out Add-Ons no longer
  empty the set: a problem no Add-On owns leaves out the last listed
  Add-On with a `set.left_out` warning, one at a time. A joiner uses the
  same rule (`Catalog::load_dirs_skipping` in `mods::load_fetched`): a
  downloaded Add-On that does not load on that PC is named in the console
  and left out of what it shows, and the join goes ahead. Test:
  `samples.rs` `add_ons_that_clash_are_left_out_one_by_one_and_never_empty_the_set`.
Merge with main 1286fc3f6: the exe's 80 ms client gate replaces the
bandwidth lane's one-sound-per-100-bricks grouping (300b1530) as the only
break-sound rule; a 250-brick blast is now one sound. Evidence: `cargo test
-p bri-client --lib audio -- --include-ignored`, `cargo test -p bri-sim
--test tools --test brick_damage -- --include-ignored` green.
## 2026-09-28 Standalone BlocklandReImagined.exe

- Max asked for one exe he can drop anywhere and run. New crate
  `crates/launcher` builds `BlocklandReImagined.exe`: the packager appends
  the release zip to it; on start it checks the zip's SHA-256, unpacks it
  into `%LOCALAPPDATA%\BlocklandReImagined\Game` and runs the game there with
  that per-user folder as its state folder. Settings, saves and Add-Ons stay
  per user, never beside the exe. Same exe again: reuses the install. New
  version: base files replaced, player files and package choices carried
  across, old game still running: asks the player to close it.
- `package_playtest.ps1` now also writes `<release>.zip` (forward-slash
  entries; Windows PowerShell's `CreateFromDirectory` writes backslashes)
  and `<release>-standalone\BlocklandReImagined.exe`, and verifies the exe
  (`-VerifyStandalone`). `-NoStandalone` skips it. The launcher must be built
  beside bri-client: `cargo build --release -p bri-launcher`.
  `package_playtest.sh` (Linux) is unchanged.
- Evidence: `cargo test -p bri-launcher` (first run, reuse, upgrade keeping
  Add-Ons and choices, damaged payload); clippy -D warnings;
  `Test-PlaytestPackaging.ps1` (zip, exe, damaged exe refused); the real
  packager on a16's exe and content: 3261 files, 86 MB zip, 87 MB exe,
  verified; `release_smoke standalone_exe_unpacks_per_user_and_starts_the_game`
  with `BRI_STANDALONE_EXE` passed (4.4 s first unpack, `--check` passed from
  the install). Not covered: a signed exe, and an interactive start (Max's).
## 2026-09-28 Slides: players move on v20's 32 ms tick (protocol 43)

- Max: movement is close to v20 except on Mr. Block's slides. Read
  `updateMove` in the exe (0x5AE2A0) against the motor: slope contact, the
  0.002/0.0021 rest, run projection, air control (0x5AF4C0: no braking with
  no input), resistance and drag (0x5AFF70 onward) already matched. The
  difference is the tick. The crease rule re-aims a wedged rider's whole
  speed once per tick, so lane speed is per tick: a rider wedged in a level
  lane settles at 6.517 u/s under v20's per-tick equations, 3.248 at 120 Hz,
  and the ride from the top of the Slides tower stopped 39 units down
  instead of 353. The motor now runs whole 32 ms ticks inside the 120 Hz
  steps (1/3000 s phase counter, exact), `PlayerState::tick` carries the
  phase and previous feet, and clients draw `shown_feet()` between ticks.
- Also v20's: the step probe from the backed-off box (the 120 Hz
  from-contact probe hopped players up 25 degree ramps at 32 ms), no -80
  fall-speed clamp (not in v20; falls reach 199 u/s under drag), the jump
  window of 8 Torque ticks (canJump 0x5A2AF8). Players already touching
  head-on part along the least-overlap axis instead of the shape cast's
  arbitrary normal, which slid one into the other.
- Feel changes that are v20's: rest 0.01 above floors, run 6.978 u/s, swim
  3.41 u/s, jump/crouch take effect on the next tick (up to 32 ms).
- Evidence: `cargo test --release -p bri-sim --test player`
  (`a_wedged_rider_gains_lane_speed_on_v20_ticks`: 6.518 vs 6.517),
  `BRI_SLIDES_FULL=1 cargo test --release -p bri-sim --test slides --
  --ignored` (120 Hz steps visit exactly the 32 ms tick positions; tower
  ride falls 366 in 10 s; 891/893 lane rides finish, was 889; 0/3570 faces
  hold), motor/sim/vehicles/net suites, `bri-client` lib and tests, clippy
  `-D warnings` on the touched crates. Not seen in a window: Max's playtest.

## 2026-09-28 Showcase Add-Ons: the Gravity Gun and the Steel Ball

- Max asked for two Add-Ons that show what modding here can do that
  v20/Torque could not, shipped turned on in the release (his later
  message: first-class, polished, fun). Both are in `packages/showcase/`
  and use only the public modding surface: data, Rhai rules, WebAssembly
  and WGSL. Where the surface lacked a seam, a general one was added to
  the engine; no gravity-gun or ball code is in an engine crate.
- Gravity Gun (`gravity-gun` rule, `gravity-gun-tool`, `gravity-gun-fx`):
  right click grabs or drops, left click punts, held left click charges a
  throw (full at 0.75 s, 24 to 70 u/s scaled by weight). It moves
  players, every vehicle and entities; thrown players tumble. Tuning:
  hold response 25/(1 + mass/150) per second, so tanks lag and sag.
- Steel Ball (`steel-ball` rule, `steel-ball-kit`, `steel-ball-fx`): a
  Rapier sphere of radius 1.25 and mass 900, no seats (spawn-brick list
  too). Left click rolls one out (11 u/s), right click hurls it (24 u/s),
  three per player. Runs players over at speed x 4 in minigames and bowls
  them into a tumble everywhere; a hit at 10 u/s or more breaks bricks up
  to volume 30 within 1.2 units under the rocket's rules.
- Engine seams: the `physics` script capability (`push`, `tumble`, `hold`,
  `let_go`, `held`, `object`, `objects`, `objects_near`, `spawn_vehicle`,
  `remove_vehicle`; `aim()` reports the movable object before the brick,
  and players expose eye, look, velocity and item); who may move what
  follows the minigame rules, trust outside them (`session/movables.rs`);
  movers are credited for 5 s, so run-overs and smashes by a thrown
  vehicle are theirs; Add-On vehicles count toward the vehicle limits.
  Images gain `commands` (per state script, and `jet`). Vehicle data gains
  `smash` and `shove`. Client code gains `world.read` (players, vehicles,
  public Add-On state as drawn), `environment`, `draw_with`,
  `material_blend` (glow and see-through layers) and `sound_at`; the game
  now plays Add-On sounds at all (decoded on start, placed in the world,
  effects channel). Sounds are generated by `tools/make_showcase_sounds.py`
  (grab, drop, charge, launch, punt; the ball's clank and thud, heard when
  its speed changes sharply).
- Sync: the server decides everything; motion rides the existing player
  and vehicle pose streams; each gun adds six numbers of public Add-On
  state sent on change. No wire protocol change (content formats gained
  optional fields only; base packs and their fingerprints are unchanged).
- Release: `package_playtest.ps1` ships `packages/showcase` turned on in
  `content/addons/`, and gives client-code-only Add-Ons the `client`
  side (it gave them `server`).
- Evidence: `cargo test -p bri-sim --test showcase` (grab, hold, swing,
  drop; charged throw vs tap; a thrown crate kills in a minigame and
  credits the thrower; trust gates grabbing outside minigames; punt
  tumbles; balls roll on and keep three per player; smash follows rocket
  rules, slow rolls break nothing; bowling shoves without harm outside
  minigames and hurts inside), `cargo test -p bri-client-sandbox --test
  showcase -- --include-ignored` (modules built from their text; effects
  and sounds follow state; offscreen renders on an RTX 4070 SUPER),
  `cargo test -p bri-client --test showcase_sounds`, the loopback test
  in `crates/net/tests/showcase.rs`, and the sim, net, package-runtime,
  client-sandbox and client suites. Not seen in a window: Max's playtest.
- Follow-up (same day): right click with a tool whose image has a `jet`
  command no longer jets on any player type: host and prediction both run
  the motor through `prediction::motor_input`, while the press still
  reaches the host as the trigger (`right_click_with_the_gun_grabs_without_
  jetting`, `a_tool_that_takes_jet_is_predicted_without_jetting`). Client
  code reads creatures too (`entities`, `world.read`), so a held creature
  gets the beam and bubble (`a_held_creature_gets_the_beam_and_bubble_too`).
- `hardening_packages` `many_heavy_thinks_keep_the_tick_budget` failed once
  in the gate and passed alone. Not an overrun: the per-tick script work cap
  counts operations, so the work is the same every run. The test timed the
  tick by wall clock, which counts time the OS gives other processes. Here
  the tick measured 12-24 ms alone and up to 37 ms beside CPU-busy
  processes; with the whole file under 8 busy processes the old
  wall-clock checks (thinks, chunk generation, player commands) failed 59
  of 60 runs. `timed()` now reads the test thread's CPU time
  (`GetThreadTimes` on Windows, `CLOCK_THREAD_CPUTIME_ID` elsewhere), where
  package scripts run; the 50 ms bound is unchanged. After: 0 failures in
  60 loaded runs, worst 25 ms. With the per-tick cap switched off the test
  still fails (1.4 s of CPU in one tick).
## 2026-09-28 Map lamps light players and bricks (client-only, no network)

- Report: on the lamp's bars in BedroomDark a player is a black silhouette.
  Evidence: neither Bedroom mission has a light object. The lamp is only in
  `bedroom.dif`'s baked lightmaps, and `lightBulbA` is a breakable `Glass`
  shape. BedroomDark and KitchenDark author a black sun and ambient, so
  vertex-lit meshes (players, items, vehicles, bricks, map shapes) got no
  light anywhere except from player lights. Of the 14 stock missions, none
  has a point light object; only the four interiors' lightmaps carry lamp
  light (Bedroom, BedroomDark, Kitchen, KitchenDark, plus Tutorial's).
- The engine family lit a shape with the lightmap colour of the interior
  surface under it (`SceneObject::getLightingAmbientColor`, OpenMBG pinned
  commit). No trace of it was found in `blocklandv20.exe`: its terrain scale
  constant (255/31) and the commented 0.57735 ambient direction are absent.
  So v20 itself most likely left players black here. Default chosen for
  player satisfaction: light them.
- `bri_render::light_volume` bakes a grid from a map's lightmapped surfaces
  on a background thread after the map loads (cells of at least 2 units and
  at most a million: 4.7 units for Bedroom, 2.5 s; Kitchen 3.2 units, 5-6 s).
  Each cell holds the brighter of the lightmap under it and the mean
  lightmap over 48 directions, so the Bedroom lamp's lit shade lights a
  player on its unlit bars. Cells inside walls hold no light, so filtering
  never pulls light toward black. Vertex-lit surfaces take the brighter of
  their sun lighting and the volume, with a 0.7 + 0.3 form term. Maps with a
  bright sun keep their look; nothing gets darker; outdoor maps have no
  lightmapped surfaces and bake nothing.
- Breaking the bulb keeps v20's rule (burst, no sound, no respawn until the
  mission reloads). The room's light stays because it is baked; the volume
  follows the room, so objects stay consistent with what the walls show.
  Turning the lamp off would need a lamp-free relight of `bedroom.dif`.
- Evidence: `cargo test -p bri-render --release --test light_volume`
  (a lit floor and a lit shade light a stand-in; dark rooms stay black;
  walls hold no light; the cell budget holds; the shader matches the CPU
  mirror; a dimmer volume never dims daylight), and with `--ignored`:
  every stock map bakes in under 30 s with the Bedroom bulb and Kitchen
  lights lit around them; an offscreen BedroomDark render of a player-sized
  box on the lamp bars goes from black (0,0,0) to (197,197,197).
  `cargo test -p bri-render --release`, `cargo test -p bri-client --lib
  --release`, clippy `-D warnings` on both crates.
- Follow-up (gate sent 9b37a574 back): the stock-map test's 30 s
  wall-clock bound failed in the gate's debug build (Bedroom 42 s, every
  cell casting 49 rays). The bake now casts 24 directions and does it only
  where needed: it bakes every 8th cell, interpolates blocks that touch no
  geometry and whose open corners agree within 12/255, halves failing
  blocks down to 2, and bakes what is left. The one ray down still runs for
  every cell, because it changes sharply over a small lit block. Casting
  from every cell remains available (`Baker::bake_every_cell`) as the
  reference: on a test room with a small lit block, the fast bake casts
  under half the rays, its mean texel error is under 1/255 and its worst is
  at most 24/255. Release timings on this PC, one test thread: Bedroom
  0.39 s (8.0 M rays of 20.6 M), BedroomDark 0.38 s, Kitchen 1.66 s
  (11.8 M of 19.7 M), Tutorial 0.18 s; before this change they were 2.5 s
  and 5-6 s. The client starts the bake as soon as the map's scene is read,
  so it runs alongside the rest of loading. It stores the result under
  `<state>/light-volumes/<sha256>.lightvolume`, keyed by the bake format,
  its settings, every lightmapped triangle and lightmap. Later loads read
  it instead of baking, and bad or foreign files are refused and rebaked.
  Tests bound work by rays cast, not time: under two thirds of casting from
  every cell on each stock map. Debug build: `cargo test -p bri-render`
  passes (69 s including the build); the ignored light-volume map tests
  pass in 52 s.

## 2026-09-28 Live sun from azimuth and elevation

- The live renderer lit every map from the mission's `direction` field, a
  stale dynamic field Torque never reads: twelve stock maps carry the same
  0.577 0.577 -0.577, Bedroom Dark's points straight up and Slopes and
  Tutorial have none. v20's Sun uses `azimuth`/`elevation`, as our bake
  already did (it reproduces the reference caches only that way). Both now
  share `bri_content::scene::sun_direction`; the bake keeps libm sines.
- Shading, cascaded sun shadows and the water highlight move on every map.
- Evidence: `cargo test -p bri-render -p bri-content -p bri-convert`, the
  content-gated `map_sun` test over all 14 stock maps, clippy clean.

## 2026-09-28 Knocked-out brick debris no longer hitches

Max saw frame hitches in a16/a17 when a dozen or so bricks broke into
debris. Cause: every new debris look (definition, paint, FX, print) built
its own scene through `build_world_scene_materials`, which carries all five
brick surface textures (plus the print). Each upload copied them, built
their mip chains on the CPU and created textures and bind groups, all on the
frame the bricks died. Looks now build against the shared `BrickPalette`
the world chunks use (`world_chunks::build_brick`) and upload geometry only
(`SceneRenderer::upload_palette_model`, a chunk upload that keeps no
model-space bounds so the shadow pass never culls moved instances). Debris
behaviour is unchanged: solid 3 s, 2 s fade, cap 128, client-only.

New headless `debris_probe` (release, Golden Gate, 44,465 bricks, offscreen
GPU) replays the client's per-frame debris work: query-mirror sync, the
hidden-brick ghost rebuild with a wand out, cues, physics, model upload and
an instanced draw. Worst frame, before -> after (machine at 100% CPU from
other builds, so absolute times are noisy; best of repeated runs):

- Rocket, 12 bricks: 48-58 ms -> 2.9-3.3 ms (model upload 46-55 -> 0.2-0.4 ms).
- Destructo Wand chain, 40 bricks: 16 ms (13 ms per new look) -> 3.5-4.7 ms.
- 128 bricks at once, then 64 more evicting: 330 ms (models 310 ms) ->
  3.7-9.6 ms (kill frame: physics 2.5-6 ms; the rest is fading draws).

Measured and not the cause: Rapier body setup (cues <= 0.9 ms for 128),
surroundings/physics (<= 2.6 ms at 128 bodies on a quiet run), query-mirror
sync (<= 0.4 ms), break sound (already one per gap), per-body draws (one
instanced draw per look). Chunk remeshing runs off-thread. Secondary: with
a building tool out, each kill rebuilds the hidden-brick ghost scene by
scanning the whole world, 0.7-2 ms on Golden Gate.

Guard (no wall clock): `brick_debris::tests::debris_looks_upload_geometry_only_against_the_brick_palette`
checks plain, translucent, printed and FX looks carry no images, index the
palette's materials and match the old standalone geometry exactly.
`DebrisModels::diagnostics` counts looks built and images uploaded (probe:
4/12/25 looks, 0 images). Evidence: `cargo test -p bri-client --lib`
(170 passed), clippy on bri-render and bri-client clean, `cargo run
--release -p bri-client --bin debris_probe -- <content> <report.json>`.
Not seen in a window: Max's playtest.

## 2026-09-28 GitHub Actions release builds

Max asked for prebuilt downloads. `.github/workflows/release.yml` runs on a
`YYYY-MM-DD-*` version tag (or a manual run with a version) on windows-latest:
fetch content, release build of bri-client, bri-import-addon and bri-launcher
with `BRI_VERSION`, `--check`, `package_playtest.ps1 -StressLab` and
`-VerifyPackage`, the standalone-exe release smoke, then a GitHub Release with
`BlocklandReImagined.exe` and the zip. Symbols are a 90-day workflow artifact.
Details and Max's setup: `docs/release-builds.md`.

Content source: a draft, prerelease GitHub release `ci-content` in this repo
holding `ci-content.zip` (only the packs the game loads), made by
`python tools/ci_content.py upload`. Chosen over a secret (48 KB limit) and a
second private repo plus token (two setup steps): drafts are private to people
with push access, the workflow's own token reads them, and assets allow 2 GiB.
Prerelease keeps a mistakenly published draft out of `releases/latest`.
`ci_content.py pack` was checked here on a stand-in content folder (zip
layout, missing-pack error). Not yet run on GitHub: needs Max's upload first.
The loopback-join smoke stays on the PC (original v20 Add-On archive, GPU).

## 2026-09-28 Brick tops world-aligned like v20

Max saw brick tops forming swastika-like pinwheels. brickTOP is v20's
bevelled-square overlay (our copy is byte-identical), and its per-brick
mapping matched the emulated generator. The cause was in the quad emitter:
v20 turns TOP UVs by each brick's angle ID so every stud's lit bevel faces
the same world direction; we kept the datablock UVs, so bricks placed at
different angles disagreed and their corners made pinwheels. The emitter's
table is now ported (`docs/audits/bricks.md` finding 10). This also turns
angle-0 tops 90 degrees from before, as v20 does.

Evidence: `cargo test -p bri-client --lib world_scene` (new
`top_studs_stay_world_aligned_at_every_angle_like_v20`), new ignored
`brick_top_audit_scene` offscreen render (0.85/255 against
`tools/brick_reference.py`; families 1.12, fx 1.26), clippy on bri-render and
bri-client clean. Not seen in a window: Max's playtest.

## 2026-09-28 Third-person vehicle camera (branch `claude/vehicle-camera`)

Playtesters called the third-person camera buggy in a Jeep. Ours orbited a
pivot 7.5 above the vehicle with the mouse's pitch (and, for mouse-steered
drivers, with the vehicle's own pitch), and gave passengers the vehicle's
13-unit camera. Read from blocklandv20.exe: the driver's control object is
the vehicle, so `Player::getCameraTransform` (0x5ab7d0) hands third person to
`Vehicle::getCameraTransform` (0x56cc10), which Blockland rewrote. With
`cameraRoll` off (every stock vehicle) it keeps the camera level behind the
vehicle's heading: the vehicle's transform, or with `$mvFreeLook` held the
rider's head turned by head yaw and pitched by `cameraTilt`, levelled to its
horizontal heading (`getCameraParameters` 0x56b440 returns an identity
rotation). The camera goes `(cameraMaxDist - cameraMinDist) * pos` back from
the world-box center; its height over the vehicle origin is `cameraOffset`
times its level distance over `cameraMaxDist`; it looks along the heading
with `-cameraTilt` as the vertical component (atan 0.4 = 21.8 degrees for the
Jeep). A ray from 2 above the box center to 1.1x the camera (mask 0x300000c:
terrain, interiors, bricks) places it: a hit before the camera puts it at the
hit, backed off by `0.8 * max(1 + n.d, 0.05)`; a hit in the extra tenth eases
that back-off in. `cameraLag`/`cameraDecay` are never read, so the client's
trailing offset is gone. Passengers keep `Player`'s camera (no control
object, 0x5a7480 returns the player's own camera fields), now around their
seat. Gunners and player-type mounts are unchanged.

Code: `crates/client/src/vehicle_camera.rs` (`driver_view`), used by
`App::view_camera`; `Controls::free_look`. Defaults picked: the world box is
the converted shape bounds' center (not the DTS header box); the free-look
head pitch is left out of the levelled heading (it only matters with the
vehicle rolled); vehicle scale is not applied (the client does not know it);
the passenger camera does not tilt or roll with the seat (v20's does; our
view has no roll). Evidence: `cargo test -p bri-client --lib vehicle_camera`.
Not seen in a window: needs Max's playtest in a Jeep, driving and as a
passenger.

## 2026-09-28 Skis usable again after jetting off (branch `claude/ski-stuck`)

Max reported skis that stop working: the player walks around holding them,
firing does nothing, and self-delete does not help. Cause: jet queues the
ski dismount, then the vehicle step deletes the now-empty skis before that
dismount is applied, so the handler could no longer tell it had left skis
and never cleared the weapons runtime's `skiing` flag. With the flag stuck,
`SkiWeaponImage::onFire` never starts skis again, and respawn kept the
flag. Fix: jetting off skis clears skiing at once (`vehicle_input`), and a
respawn clears it too, since skiing belongs to the old Player object.
Host-only state, so single player and multiplayer share the fix; no
protocol change. Evidence: `cargo test -p bri-sim --test vehicles
skis_work_again -- --ignored` fails before the fix and passes after;
`cargo test -p bri-sim -- --include-ignored` passes.
## 2026-09-28 Pong "Event color outside palette" report

Max's Pong crash ("Invalid replicated brick 76: Event color outside
palette") was on a18 (29a600a77; session log 20260928-182915, 74 minutes
into a hosted game). It is the paint-fade bug fixed in a19 (5ccf50c6,
f30e8019): Pong's `setColor` rows repaint bricks, the fade drew a repainted
brick in a one-colour palette but kept its event colours. The host was
checked and keeps no such brick: every Demo Pong test now validates every
brick against the palette after each tick (reset, rallies, scoring, win,
hammered paddle buttons, timed reverts). Evidence:
`cargo test -p bri-sim --test pong -- --ignored` (6 passed). No host change
was needed.
## 2026-09-28 Joined players no longer fall through far terrain

Max reported clients far from the host (on Slopes) falling through the floor.
Cause: terrain collision streams in 512-unit tiles around every moving
body's position. A joined client predicts its own movement in a collision
mirror that is queried but never stepped, so its kinematic body never
reaches the pose the motor sets and stays at the join point. Past the tiles
loaded around that point (about 700 units on Slopes-size tiles) the
prediction had no ground and fell, fighting every server correction. The
host was unaffected because its world steps. Fix: `body_foci` also covers a
kinematic body's target pose (`crates/physics/src/terrain.rs`). No protocol
change. Evidence: `cargo test -p bri-sim --lib
predicted_players_stand_on_terrain_far_from_where_they_joined` fell to
y = -3782 at 800 units before the fix and stands on the ground after it;
`cargo test -p bri-physics -p bri-sim`.
## 2026-09-28 Flying vehicles and vehicle Add-On fields (branch `claude/project-thread-e0lly9`)

A playtester ported the Stunt Plane (Kaje and Ephialtes, a community
`WheeledVehicleData`) but had to edit the engine: no way to spin its
propeller, flying fields missing. Checked against the Add-On's scripts, the
recovered core scripts and blocklandv20.exe:

- Blockland's flying forces run in every `WheeledVehicle::updateForces`
  (0x5746a0), driven by the datablock's fields. We gave them only to a
  `FlyingWheeled` family the stock importer assigned to the Flying Wheeled
  Jeep by name, and read their fields from `authored`. Any imported wheeled
  Add-On drove as a car. Now schema 6 types them (`wheeled_flight`,
  `steering`) and the `FlyingWheeled` family is gone: a `Wheeled` or `Skis`
  vehicle with `wheeled_flight` flies. The importer sets it when any flying
  field is nonzero. `Pack::load` upgrades schema 5 packs (the stock
  vehicles-pack-011 and earlier imports) from `authored`, so no content
  regeneration is needed; the stock converter now writes schema 6.
- `WheeledVehicleData::onAdd` chooses steering and driven wheels by wheel
  count; we used the Jeep's rule for all. Now the table, and the Add-On's own
  `onAdd` `setWheelSteering`/`setWheelPowered` calls (without
  `Parent::onAdd`, Torque's defaults: no steering, powered).
- Vehicles could not play model animations. Schema 6 `threads`: slot,
  sequence, `rate` (negative is `setThreadDir(slot, false)`) and an optional
  speed range. The importer reads `playThread`/`setThreadDir` from `onAdd` and
  the functions it hands the object to, and turns `if (%speed < n)` with
  `%speed = vectorLen(%obj.getVelocity())` into ranges; the Stunt Plane gets
  `propslow` below 5 and `propfast` from 5. The client draws the parts those
  sequences move apart and poses them from the server tick.
- `FlyingVehicle::updateForces` (0x568770) and `getHeight` (0x568420) are
  stock Torque. Ours multiplied the damping surfaces by speed; fixed. The
  hover support (90% of the weight above the 10-unit band) was already right.
  Every force and torque is along the craft's own axes, so it climbs where the
  nose points; tests now pin that.
- The DTS reader refused a sequence whose empty trigger list keeps a stale
  start index, so the Stunt Plane's model did not convert.

`cameraRoll` stays unmodelled: every known vehicle, the Stunt Plane
included, sets it false. The Ball (a `WheeledVehicle` with flying fields in
v20) keeps its own family and gets no flying forces, as before.

lpsroo's own patch (pasted by Max) made the same calls: flying by nonzero
thrust, lift or surface fields, v20's wheel table, speed-switched looping
threads read from `onAdd` and its helpers, the DTS empty-range fix, and a
spinning part drawn apart on the client. Taken from it as well: an image
state over 300 s (the contrail images wait 10000 s) is reported and left
out instead of failing the whole weapons pack, and a thread naming a
sequence the model lacks is dropped. It kept a `FlyingWheeled` family and a
separate `speed_threads` list; here the fields are typed on `Wheeled` and
threads carry slot, rate and both speed bounds.

Defaults picked: a speed-switched thread changes the moment the speed crosses
(v20's script checks every 2 s); a non-looping thread holds its end; a
thread's model parts must not be skinned; the propeller's phase comes from
the server tick. Evidence: `cargo test -p bri-vehicles` (new
`tests/flying_vehicle.rs`: schema 5 upgrade, the carpet climbing and diving
along its nose, mouse pitch about its own wing when rolled),
`cargo test -p bri-addon-import` (`vehicle_script` unit tests;
`real_community_samples` imports the real Stunt Plane when the archive is on
this machine and checks its fields, wheels, threads and that it holds height
on lift at 45), `cargo test -p bri-client --lib threads_pick`. Not seen in a
window: the propeller and how the plane and carpet feel need Max's playtest.

## 2026-09-28 Brick debris limit in Options (branch `claude/project-thread-76ojrm`)

Max wanted more knocked-out brick debris, with each player choosing how much
physics their PC takes. v20's Graphics pane already had a Physics Quality
section (`OPT_PhysicsQuality0..4`, `$pref::PhysicsQuality`, stock default 1
High) that this client hid. It is shown again beside Shadow Quality and sets
the debris limit, stored as v20's own `$pref::Physics::MaxBricks`:

| Physics Quality | Off | Low | Medium | High (default) | Best |
|---|---|---|---|---|---|
| Debris bricks at once | 0 | 128 | 256 | 512 | 2048 |

The console's `maxdebris` sets any other limit, 0 to 4096; Options then shows
no radio selected and keeps it. Off throws nothing: bricks still die and
vanish. Still client-only, nothing sent, no protocol change.

A CPU budget keeps a weak PC from hitching: the client times its debris work
each frame (cues, pushes, physics), learns what one moving brick costs on
this PC (skipping the frame that threw them, which pays for the spawn), and
keeps only as many as 6 ms pays for. Two frames running over 6 ms, the oldest
beyond that go. A disconnect keeps what it learned. Bricks removed early
(over the limit or the budget) no longer pop: they stop colliding and fade
over 0.35 s where they were heading, unless no frame drew them yet. Also
fixed: every brick one blast killed re-shoved the debris already flying, so
a 40-brick blast pushed older debris 40 times, to top speed.

`debris_probe` now costs a big blast at each limit in this thread's CPU
cycles (`QueryThreadCycleTime`, converted with the run's thread CPU time),
not wall clock, with work counts (bodies, moving, touching pairs, solid
surroundings). Golden Gate at its middle, 4.2 GHz, 6 ms budget; "budgeted"
is a second blast after the first taught the budget:

- Stock rocket (radius 5, 38 bricks): 1-2 ms peak, 0.3-0.5 ms average, any
  limit.
- 1024 bricks in one blast: Low 5.6 ms peak / 1.7 ms average over the first
  second; Medium 7.3 / 2.9; High 9.6 / 5.3; Best raw 20 / 10.4, budgeted
  12.7 / 5.6 holding 868.
- 4096 bricks: High 12.7 / 4.2; Best raw 32 / 17.2 (2048 bodies, 3248
  touching pairs), budgeted 14.9 / 5.8 holding 767; 4096 raw 89 / 51,
  budgeted 16.2 / 5.9 holding 933.

About 7-10 us per moving brick per frame here, 2 physics steps (120 Hz).
The first blast of a session can still hitch once at Best on a slow PC,
before anything is learned.

Evidence: `cargo test -p bri-client --lib brick_debris` (13 passed; new:
limit and Off, learned budget and shedding, fade-out ghosts, one shove per
blast), `cargo test -p bri-ui --lib options` (Physics Quality radios, Done,
console value), clippy on bri-client and bri-ui clean, offscreen
`ui_runtime_probe` render of the Graphics pane, `cargo run --release -p
bri-client --bin debris_probe -- <content> <report.json>`. Not seen in a
window: how blasts look and feel at each preset is Max's playtest.
## 2026-09-28 Joins no longer refuse over Add-Ons (branch `claude/join-addons`)

lpsroo, on a20, could not join Wilfred's host: "can't join unless I have
Add-Ons". A join downloaded what the host offered and asked again, and the
host refused that second join over anything still different. Three things
reached that refusal: base game content that differs between two installs
(the host never offers it), an Add-On the host cannot send (a file servers
never send; the host then could not host at all), and a download or load
that failed. Max's rule is that a join downloads everything it can and
never fails over Add-Ons.

Now the join after downloading carries `accept_differences` (protocol 49):
the host lets the player in with whatever still differs and tells them in
chat, by package id and version, what they joined without. A package the
host cannot send is left off its download shelf, not fatal to hosting. A
download that fails, or a package that is unsafe or fails to install, is
left out and the join goes ahead; so does a downloaded set that fails to
load (the join retries with none) or whose bricks, weapons or vehicles fail
to load in the game (the player joins without them and is told in chat).
The first join still refuses once, which is what tells the joiner to
download.

Default picked: let the player in even when base game content differs.
The host is authoritative; what the joiner lacks may look or behave
differently, and the chat line says so. The Can't Join dialog now only
appears against hosts on older builds.

Evidence: `cargo test -p bri-net -p bri-package` (new
`package_sync::a_join_goes_ahead_without_content_the_host_cannot_send`,
which first reproduces the a20 refusal after downloading and then joins
with the downloaded Add-On, the other two named in chat;
`a_failed_download_joins_without_the_add_ons`), `cargo test -p bri-client
--test add_on_fallbacks`, and the ignored
`add_on_join::a_guest_joins_a_host_running_every_repository_add_on`
(host with every repository Add-On, the showcase ones included).
## 2026-09-28 Riding horse players (branch `claude/project-thread-c06rfc`)

Max's a19 playtest: the Horse Ray turned him into a horse and the other
player could not get on. Only horse bots (vehicles) were mountable; nothing
mounted a player.

v20 rules, from `Armor::onCollision`, `onMount`, `doDismount`,
`onNewDataBlock`, `onDisabled` (allGameScripts-Vanilla.cs 8840-9160) and
Vehicle_Horse/Weapon_Horse_Ray: a `canRide` player whose feet are more than
0.2 over a `rideable` player with `numMountPoints > 0` takes its first free
mount node, `$Game::MinMountTime` after leaving any mount. HorseArmor has one
seat, `mountNode[0] = 2` (`mount2`), `mountThread` root. The horse's own
client keeps control; the rider is a passenger who looks around, uses tools
and leaves with jet (2.2 up, else 3 up/down/sideways, times the mount's
scale). A mount without a client (a Horse-Rayed bot) is steered by the rider
in its first seat. Permission: a player has no spawn brick, so only
`miniGameCanUse` counts (anyone outside minigames, same-minigame players
inside); a bot asks its spawn brick owner like its vehicles. Death,
disconnect, respawn, a body without seats and `canRide` loss put riders down.

Made data, not a Horse case: `Archetype::mount_points` (node, rest position,
pose), v20's horse from its shape; package archetypes declare
`mount_points`. Server `session/riding.rs` seats, follows, dismounts and
cleans up; riders are sensors like vehicle riders. Replicated as
`Vitals::ride` (mount, seat, steers); protocol 49. Clients show the host's
rider pose on the mount's animated node, lock a passenger's facing to the
mount, and leave riders out of prediction's other players.

Players never quite touch (each closes half its gap per tick), so a rider
counts as touching within 0.1; at 0.05 a rider landing on a walking horse
slid off without mounting.

Add-On `PlayerData` is still not importable (addon-import), so Add-On
player types with mount fields wait on that importer.

Evidence: `cargo test -p bri-sim --test unlike_modes` (riding a horse player,
the horse's prediction matching the host, jet dismount, cleanup on body
change/death/rider death/disconnect, two-seat package mount, minigame
refusal); with content, `cargo test -p bri-sim --test vehicles --
--include-ignored` (horse seat equals horse.dts `mount2`; a Horse-Rayed bot
ridden and steered east). Also `cargo test` for bri-sim, bri-motor,
bri-package-runtime, bri-net, bri-client; clippy clean. Not seen in a
window: how mounting and riding feel needs Max's playtest.
## 2026-09-28 Old v20 saves convert themselves (branch `claude/project-thread-3otez8`)

Players can't have their own v20 `.bls` saves converted ahead of time, and
the game could not read a `.bls` at all: Load Bricks listed only the stock
saves converted offline and saves made in this game. Now any `.bls` in the
saves folder (`<state>/saves`: `user-state` beside `Launch.cmd`, or
`%LOCALAPPDATA%\BlocklandReImagined` for the standalone exe) converts on a
background thread on the first frame, and again whenever Load Bricks opens,
so a file dropped in mid-game shows up too. Wilfred's suggestion via Max:
automatic at startup, nothing for the player to do.

- Layout: v20's `saves/<Map>/<name>.bls` copied in whole, or loose files.
  A folder names its map the way v20's `saveName` did (Bedroom, Kitchen,
  Slate, Slopes, Tutorial; every Slate variant saves as Slate). An unknown
  folder is listed under its own name; loose files under "Other". The
  game's own `map-<sha>` folders are skipped. v20 names ending in a space or
  dot ("Afghanistan DM ") are listed trimmed; that also makes the stock
  "Afghanistan DM" save visible in Load Bricks, which it wasn't before.
- Conversion is the offline stock-save pipeline (bricks by UI name, then
  lights/emitters, wrench events and spawn bricks, then items), against the
  loaded content including Add-On bricks (stock names win). Originals are
  only read. Native copies go to `<state>/converted-saves` with an index
  keyed by path, size, modified time and a fingerprint of the converter and
  content, so each save converts once and again only when it or the content
  changes; unreferenced copies are pruned. A save that fails is skipped and
  logged once. Load Bricks gets new saves as they finish.
- Order in the list: stock saves < converted `.bls` < saves made in this
  game, so saving under the same name keeps the original `.bls` and lists
  the new save. Colours go through the existing Color Warning path
  (`LoadBricks_GetColorDifference`), ownership stays v20-metadata as for the
  stock saves, and the Load Brick Ownership box applies as before.
- Load Bricks gets a **Saves Folder** button beside Load Brick Ownership that
  opens the folder in Explorer. An old Blockland install's `saves` in the
  usual places (Program Files, Program Files (x86), Steam, `C:\Blockland`)
  is listed read-only; nothing is ever written there.
- Pivot: `bls.rs`, `events.rs` and `effect_bindings.rs` moved from
  `bri-convert` into a new `bri-bls` crate (content, world and events only),
  so the game depends on it without pulling the Torque asset readers into
  the runtime graph. `bri-convert` re-exports them; the offline tools are
  unchanged.

Evidence: `cargo test -p bri-bls -p bri-convert --lib`;
`cargo test -p bri-client --lib saves` (folder layout, loose/unknown/old
install listing, skip on broken, convert-once, reconvert on change and on
new content, pruning, originals byte-identical, game save shadowing);
`cargo test -p bri-ui --lib saveload` (the button requests
`OpenSavesFolder`); with `BRI_CONTENT`, `cargo test -p bri-client --test
old_saves -- --include-ignored`: the game's conversion equals the offline
pack for all 35 stock saves, and dropped real saves list, load with every
brick and colour, keep Demo Pong's events and stay unchanged on disk;
clippy on the four crates. An offscreen render of Load Bricks shows the
button fitting beside the ownership box. Not seen in a window: Max's
playtest of dropping saves in and loading them.

## 2026-09-28 Duplicator chat commands and item bounds for every item (branch `claude/duplicator-cmd`)

Max, on a21: `/duplicator` did not pull the Duplicator out, and setting a
brick's wrench item to the Duplicator was refused with "Missing authored
item bounds: duplicator-tool:weapon/duplicator".

- Reference: the archived v20 Duplicator Add-On (`Tool_Duplicator.zip`, by
  Plornt; not in the v20 reference install) registers `serverCmdDuplorcator`,
  `serverCmdDup` and (packaged) `serverCmdDuplicator`, with no permission
  check: each mounts `DuplorcatorImage` on the player without a tool slot.
  Its "Admin Only" pref (off by default) gates planting, not pulling it out.
- Our Add-On already declared `/dup` and `/duplicator` (anyone may use).
  It now declares `/duplorcator` too. What stopped Max: ours lives in a tool
  slot, and with all five full `give_item` failed ("Inventory full").
  `Session::give_tool` with equip now puts the tool in hand (else the last)
  down on the ground to make room, so the command works whatever the player
  carries, as v20's slotless mount did, without losing an item.
- Item bounds, the general path: only the base weapons package ships item
  physics, so an Add-On item without an importer-made `item-physics.json`
  had none on the host. That refused wrench item spawns, build loads holding
  the item, drops of it (`Item physics catalog is not installed`), and
  pickups of a dropped one (no contact box). Two layers now:
  `ItemPhysicsContent::load_with` gives such an item its stock model's
  bounds (the Duplicator's `wand.dts`, as its presentation already borrows
  that model), else `ItemBounds::FALLBACK` (half a unit each way); and
  `Session::set_item_bounds` gives every item the server has
  (`WeaponsWorld::item_ids`, core tools included) the fallback when its
  content gave none. Unknown items are still refused.

Evidence: `cargo test -p bri-sim` (new
`slash_duplorcator_pulls_the_duplicator_out_with_every_slot_full`);
`cargo test -p bri-net`; `cargo test -p bri-weapons --lib`;
`cargo test -p bri-client --test add_on_fallbacks` (every Add-On weapon has
bounds, a stock model lends its box, an artless item gets the fallback, and
the wrench's item-spawn edit for it validates); clippy on weapons, sim and
net. Not seen in a window: Max's playtest of `/duplicator` with full slots
and a wrench item spawn of the Duplicator.
## 2026-09-28 Admin ranks and clearing bricks (branch `claude/admin-ranks`)

What a20 already had (checked first): Admin/Super Admin roles, the host as
Super Admin ("has become Super Admin (Host)"), Admin and Super Admin
passwords from Start Game, the Player List password login with v20's four
tries, kick/ban/clear bricks/admin menu gated on rank, a per-host
`administration.json` with bans and an auto-rank list. Missing: any way to
give or take a rank, a reachable auto-rank list, and other players' ranks in
the Player List (only your own showed; everyone else read as a player).

- v20 had no stock command to promote someone: ranks came from passwords or
  `$Pref::Server::AutoAdminList`/`AutoSuperAdminList` edited by hand. Native
  adaptation: the host and Super Admins make a player Admin or Super Admin,
  or take it away, from three buttons under the Admin menu's player list and
  with `/admin`, `/superAdmin` and `/deAdmin <name>`. Admins cannot. The host
  can never be demoted; bots can't be ranked. Everyone sees "X made Y Admin".
- A given rank is saved in the host's list and returns on rejoin, like v20's
  auto-admin lists. **Pivot from the brief's "keyed by name"**: the list is
  keyed by the player's verified key (the principal every client proves on
  join, a20's BL_ID), with the name kept for display. Matching on name over
  the Internet would let anyone type an admin's name and get Super Admin.
  Duplicate-name suffixes and renamed players are therefore handled
  naturally. A player without a key (none in practice) keeps a rank for the
  visit only. Old list files without names still load.
- **Saved Ranks >>** (Admin menu, host and Super Admins) lists the saved
  ranks with name, rank and key prefix; Remove takes one off after a
  confirmation (an online player keeps theirs until they leave).
- Player List shows everyone's rank as v20 did: `S`, `A` or `-`.
- Server Settings stay host-only: v20's serverConfigGui was a host-local
  prefs dialog, not a remote admin screen. Super Admins set the Admin
  password as v20's `serverCmdSADSetPassword` allowed.
- Clear All Bricks hang (Max, single player): every removed brick ran a full
  physics refresh, so clearing was quadratic (40,000 bricks: 16 s in a debug
  sim test, the host frozen and the menu on "Waiting for host" meanwhile).
  `Simulation::remove_many` removes a batch and refreshes once (0.6 s);
  Clear All Bricks and Clear Brick Group use it.
- v20 chat commands: `/clearBricks` clears your own bricks (anyone, once per
  five seconds, "X cleared X's bricks", `ServerCmdClearBricks`), and
  `/clearAllBricks` is admin-only (`ServerCmdClearAllBricks`), the same
  server path as the menu.
- Wire changes for Gate: new `Action::RequestAutoRoles`,
  `AdminData::AutoRoles`, `HostSetRole`/`HostSetAutoRole` now allowed to
  Super Admins, `AutoRole.name`. Protocol version left at main's 50 (Gate
  owns numbering; this needs the next one).

Evidence: `cargo test -p bri-admin` (grant/revoke/rejoin by key, imposter by
name gets nothing, host protected, keyless visit-only, saved list reads,
old files load); `cargo test -p bri-ui --test admin_screens` and `--lib`
(rank buttons confirm before sending, only SA/host, saved list validation
and removal); `cargo test -p bri-sim --test clear_bricks --test
hardening_session` (40k clear under 5 s, `/clearBricks` own-only with the
cooldown, Clear All admin-only); `cargo test -p bri-client --lib admin_ui`
(chat commands, key round trip, rank replies); `cargo test -p bri-net --test
loopback` (over QUIC: host makes a player Super Admin, who makes another
Admin; an Admin cannot; the file names both; a fresh rejoin with the same
key is Super Admin again, a stranger using the name is not). Release, real app, headless:
`cargo test -p bri-client --test clear_bricks_flow --release -- --ignored`
hosts Kitchen in single player, loads 20,000 bricks, opens Admin > Clear
Bricks and clears all in 0.09 s ("Action accepted by the host"). Offscreen
renders of the Admin menu and Saved Ranks (`artifacts/native-admin-ui`)
show the new buttons fitting under the list. Not seen in a window: Max's
playtest over the Internet with a second player.
Follow-up outside this lane: the brick chain kill (`debris.rs`) still
removes stranded bricks one at a time with a physics refresh each.
## 2026-09-28 Guests hammer their own spawn bricks (branch `claude/project-thread-7p7umh`)

Playtest a20 (5b476a991): an Internet guest placed a vehicle spawn, set it to
the Blockhead Bot, and could not hammer the brick back; the host could.

- Cause: v20 flags the Spawn Point and Vehicle Spawn datablocks
  `indestructable = 1`, and our `Simulation::remove` refused such bricks for
  anyone but an administrator (the hammer and wand asked the same rule
  first). The host is Super Admin, so only guests hit it. In v20 the flag
  only keeps explosions off a brick: `hammerImage::onHitObject` asks the
  chain kill and trust, nothing else, and `killBrick` removes any brick.
  Undoing a planted spawn brick failed the same way for guests.
- Fix: removal no longer checks the flag; the hammer and player wand no
  longer ask it. Explosions (`ProjectileData::onExplode` path) and the chain
  kill still skip indestructible bricks, as before.
- Ruled out with the loopback test: the brick's owner is the guest, not the
  host or the map; it stays theirs after they leave and rejoin with the same
  identity; the swing lands on the brick once the bot walks off (a swing at
  the bot itself hits the player, as in v20). The client does no hammer
  prediction. Killing the brick takes its bot or vehicle with it, like
  `fxDTSBrick::onDeath`, through the existing reconcile.

Evidence: `cargo test -p bri-net --test loopback
a_guest_hammers_their_own_bot_spawn_brick_after_rejoining` (host plus guest
over QUIC with default trust; failed before the fix with the brick still
standing, passes after, and the bot leaves with the brick);
`cargo test -p bri-sim --test tools
builders_hammer_and_undo_their_own_indestructible_bricks` (replaces the test
that asserted the old rule); with content, `cargo test -p bri-sim --test
vehicles a_guest_hammers_their_own_vehicle_spawn_and_its_jeep_goes_with_it
-- --ignored` (stock Vehicle Spawn and Jeep, non-admin guest); full
`cargo test -p bri-sim -p bri-net`. Not seen in a window: Max's playtest.
## 2026-09-28 The Stunt Plane ships as a default Add-On (branch `claude/project-thread-rwzbus`)

Max asked for the Stunt Plane as a first-class Add-On that comes with the
game. It is not in the v20 reference: its `Add-Ons` holds eight vehicles
(Ball, Flying Wheeled Jeep, Horse, Jeep, Magic Carpet, Pirate Cannon,
Rowboat, Tank). The only copy is the community `Vehicle_Stunt_Plane.zip`
(Kaje and Ephialtes, no licence file, sha256 `e68fd173…329e`) in Maxwell's
archive. Asked first because the repo treats that archive as not
redistributable; Max said yes to shipping it in public releases.

Picked: a packaged Add-On, not base content. Stock vehicles ship in the
generated `v20-vehicles` pack, but the plane is not v20's, and a base pack is
never offered to joining players (the join Add-Ons fix lets them in without
it). As an Add-On it ships like the Duplicator:
`content/addons/vehicle_stunt_plane`, turned on in the release's
`packages.json`, offered for download by hosts, and players can turn it off.

- `tools/shipped-addons.json` lists it (id, version, archive hash, vehicle
  id). `tools/shipped_addons.py build` runs Import Add-On with the v20
  reference and recovered core scripts into `content/shipped-addons/<id>`
  and checks it; bootstrap does this when the archive is present. The
  import converts with no failed assets; its `JeepVehicle.uiName = ""`
  (hiding the Jeep) stays unsupported, so the Jeep keeps its place.
- `package_playtest.ps1` refuses to package without it, copies it in and
  turns it on; `-VerifyPackage` (run by the release workflow) refuses a
  release that does not turn it on at `addons/vehicle_stunt_plane` with its
  vehicle. `ci_content.py` packs it and `fetch` requires it, so the GitHub
  release build carries it once the content zip is uploaded again.
- Upgrades keep it on: the launcher keeps the new version's own list less
  what the player turned off.

Evidence: `tools/tests/Test-PlaytestPackaging.ps1` (the fixture release
ships it on and shared; verify refuses a release without it; the packager
refuses to build without it); `ci_content.py pack` over a fixture content
root with and without it; `cargo test --release -p bri-client --test
add_on_join -- --ignored a_guest_without_the_stunt_plane_downloads_it_and_can_spawn_it`
(a host with it lists it among its spawnable vehicles; a guest with only
the base game downloads it, joins and lists it). Not seen in a window:
Max's playtest of spawning and flying it from a fresh standalone install.
Needed on the PC: `python tools/ci_content.py upload`, or release runs stop
at "Fetch the generated v20 content" asking for it.
## 2026-09-28 Event explosion loops no longer end the game (branch `claude/project-thread-xc3a5y`)

Playtest report: a friend's endless relay loop of `spawnExplosion` ended the
a20 host with "Network worker stopped". The session log held no other error.

Cause, reproduced headlessly on main (`crates/net/tests/event_storm.rs`):
- Event `spawnExplosion` spawned a stationary live projectile instead of
  exploding it (v20 `%p.explode()`). A zero-delay loop filled the host's
  1024-projectile budget within a tick and kept it full: nothing exploded,
  every player's weapons were refused, and the debug host ran about 5x
  slower than real time.
- The dialog hid the real failure. When the network worker's `run` ends, the
  movement channel closes at once, but `Event::Failed` was sent only after
  the host finished stopping and saving (and with `try_send`, so a full UI
  queue lost it). The next frame's movement send reported "Network worker
  stopped" instead. The worker also died outright when 128 presentation
  events or notices were waiting.

Fix:
- `WeaponsWorld::spawn_explosion` explodes where it is made at the start of
  the next tick without flying; at most 8 per tick (`MAX_EXPLOSIONS_PER_TICK`).
  Brick and player `spawnExplosion` use it; a fake-killed brick spawns none,
  as v20. Event-spawned projectiles are also capped at 8 per host tick
  (`MAX_EVENT_PROJECTILES_PER_TICK`), besides the owner quota. Over-limit
  spawns are dropped and the host logs the count at most every 10 s.
- Players who are not administrators get v20's `serverCmdAddEvent` floor:
  `fireRelay*` rows under 33 ms are saved as 33 ms. Administrators (the
  existing flag, so the admin-ranks lane's roles inherit it) and loaded
  builds keep zero-delay relays. This narrows the alpha-contract item
  "without an imposed 33 ms per-hop wait" to administrators, per the
  playtesters' request relayed by Maxwell; the event engine is unchanged.
- The client network worker waits (up to 5 s) to deliver `Event::Failed`,
  movement sends to a stopped worker are dropped so the real reason is
  shown, and presentation cues and notices only use queue room beyond what
  replies need (256 slots, 98 reserved); excess cues are counted as dropped.

Evidence: `cargo test -p bri-net --test event_storm -- --ignored` (debug
and `--release`): a zero-delay loop for 1200 host ticks gives 9592
explosions (8 per tick), no lingering rockets, no step errors; release ran
in real time with 73 dropped ticks. A 33 ms loop gives 302 explosions in 10
s (v20's 30 a second). On unmodified main the same loop held 1024 rockets
and produced no explosions. `hardening_session`
`relay_rows_keep_v20s_33_ms_floor_except_for_administrators`. Not seen in a
window: Maxwell's playtest of an event explosion loop on a hosted game.
## 2026-09-28 Horse and turret third-person camera (branch `claude/horse-camera`)

Max, a21: riding a horse, the third-person camera was wrong. Horse riders,
like every player-type mount's rider (rowboat, pirate cannon, tank turret),
fell through to the gunner chase camera: it orbited a pivot `cameraOffset`
over the mount's feet (2.3 for the horse) along the untilted look and only
turned the view down by `cameraTilt` afterwards. In v20 the horse is a
`PlayerData` (`Vehicle_Horse/server.cs` `HorseArmor`: `cameraMaxDist` 8,
`cameraVerticalOffset` 2.3, `cameraTilt` 0.261, box `2.5 2.5 2.4` x4) and
the rider's control object, so the view is the horse's own
`Player::getCameraTransform` (0x5ab7d0, `docs/player-simulation.md`): the
pivot is the middle of the horse's box plus 2.3 (feet + 3.5, 1.2 higher than
ours), the look is pitched down by the tilt and the camera sits 8 back along
that tilted look. The Horse-Rayed player already used that camera.

Now every `SeatRole::Actor` rider uses the same `pivot_camera` as players
on foot, with the mount's collision hull as its box and its own camera
fields. The same flaw hit the Tank's gunner: v20's `TankVehicle::onAdd`
mounts a `TankTurretPlayer` (8 / 2.3 / 0.261, box 1.7) on mount2 and the
gunner controls it, so the gunner now sees the turret's player camera from
the mount2 node instead of the Tank's 13 / 7.5 / 0.4 camera. The turret's
definition is found as the pack's player-type definition drawn with the
Tank's attachment model (`VehicleAssets::attachment_definition`). The old
chase camera only remains for a gunner seat with no turret player (none in
the stock packs). Drivers of real vehicles are unchanged.

Defaults picked: first person on a horse stays at the rider's seated eye
(the horse's `Eye` node, 2.39 over its feet, was not adopted without v20
first-person evidence); the mount's scale is not applied (the client does not
know it). Evidence: `cargo test -p bri-client --lib` (192 passed) and
`cargo test -p bri-client --lib -- camera horse --include-ignored`
(`a_horse_rider_sees_the_horse_player_camera`: pivot feet + 3.5, 8 back,
tilt 0.261; turret feet + 0.85 + 2.3; the Tank gunner resolves
`TankTurretPlayer`); clippy on bri-client. Not seen in a window: Max's
playtest riding a horse and gunning a Tank in third person.
## 2026-09-28 Tutorial target practice made real (branch `claude/project-thread-2y9r2w`)

Max, a21: the Tutorial got stuck at target practice. Cause: the practice
kept its targets as numbers on the server only. They were never sent to
clients or drawn, and shots were tested against a guessed box that a level
shot passed over. A player saw "Prepare for Target Practice!" and an empty
range; the door opened silently about 66 s later. Reproduced headless with
the real Tutorial content: 0/58 hits firing down the lanes, door 4 opening
only when the schedule ran out.

Fix, following `Map_Tutorial/tutorial.cs` (`launchTarget`, `scrollTarget`,
`checkForEnd`, `ProjectileData::onCollision` in `TutorialParentingPackage`):
- tutorial-pack-003 (schema 2) carries `target.dts`, `targetHit.dts`,
  `targetM.dts` and `targetMHit.dts` (converted by `read_dts`, byte-identical
  to the geometry pass) and their textures plus the m1-m3 skins from
  `Map_Tutorial.zip`. `regenerate_content.py` passes the archive (recipe 2).
  The pack was generated into the shared `content/tutorial-pack-003`.
- A target is a `TargetView` launch (lane, speed, datablock, skin, launch
  tick); its position follows from the tick on the server and on clients.
- Shots collide with each standing target's `Collision-1` detail through the
  weapon query (`TargetId::Shape`), stop there, and knock it down to its Hit
  datablock with `hammerHitSound`. Every projectile collision until the
  Tutorial is completed counts as a shot fired; `beginTargetPractice` resets
  the counts. A brick in hand counts as holding nothing in the prompt.
- `Checkpoint`/`Delta` carry the targets (protocol 54); the client draws them
  from the tutorial pack at the presented server tick. Without the models the
  practice still runs and completes, and a log line says so.
- Orientation was checked in an offscreen frame: the first build drew the
  boards' grey backs; the model's painted face is its +x, so the quarter
  turn is -90 degrees about up.

New gate test `tutorial_walkthrough.rs`
(`a_new_player_plays_the_tutorial_through_target_practice`): the real App
from first launch (Default Controls, Play Tutorial) through Look, Move,
Jump, Duck, Bricks, Build (a 28-brick staircase to the hole), Break, Jet,
Light, Ride (wrench the pad, horse over the water), Dismount, Wrench (light,
emitter, item), Print (OINKMOO), Diving and Shooting, using only key binds,
mouse motion, clicks, typing and the mouse wheel; it requires at least 40
target hits. Runs about 170 s of game time. Evidence: release runs pass with
51, 56 and 58 of 58 hits; `BRI_TUTORIAL_SHOT=1` saved
`artifacts/tutorial-walkthrough/target-range.png` showing red-and-white
boards moving along the lanes. Also `cargo test -p bri-content --lib
tutorial`, `-p bri-sim --lib tutorial`, `--test weapon_query` (shots stop at
shape targets), `--test session tutorial`, `--test tools tutorial`,
`-p bri-net --test replication` (targets replicate whole; invalid lanes and
duplicate ids refused), clippy on the touched crates.

Next: extend the walkthrough with Drive, Spray, the wand room, the finish
and the optional Secrets. Not seen in a window: Max's playtest of the
target practice.
## 2026-09-28 Brick item respawn ghost (branch `claude/project-thread-2vmevi`)

Max: picking up an item a wrench put on a brick left nothing behind; v20
kept a ghost of the weapon at the brick until its respawn timer brought it
back. v20 (`.research/bl-decompiled/v20/server/scripts/allGameScripts.cs`):
`ItemData::onPickup` (7332) and `Weapon::onPickup` (7702) call
`Item::Respawn` for a static item (7409, 7786), which runs `fadeOut` (7228:
node colour `<ItemData colorShiftColor rgb or white> 0.25`, `canPickup = 0`)
and schedules `fadeIn` after the brick's `itemRespawnTime` (7267; 1000..300000
ms, 4000 default). `fadeIn` (7243) restores the image colour and
`canPickup = 1`; a minigame reset calls `fadeIn(0)` (22292/22310). Node
colour is networked, so every player sees the ghost. The same script also
calls `startFade(0, 0, 1)`, which in the TGE family would hide the shape
outright, so `docs/runtime-world-items.md` had left the ghost unresolved;
Max's own v20 observation settles it, and the node colour decides.

The server already enforced the wait and replicated only the availability
tick (`StaticItem::available_at`); the client skipped drawing a waiting
item. `world_items.rs` now draws it as the ghost from that tick: ItemData
colour, instance alpha `RESPAWN_GHOST_ALPHA` 0.25, translucent and without
a shadow (the renderer's existing faded-instance path). No wire change;
protocol stays 52. One server gap fixed on the way: `fxDTSBrick::setItem`
(11324) deletes the Item and creates a fresh one, and the wrench's Send
always sends `IDB` (client `wrenchDlg::send`, server 11025), as does the
`setItem` event, so both now restock a faded item
(`ItemSpawners::restock`). Direction/position/respawn edits keep the clock
as before (`setItemDirection` etc. move the same Item).

Evidence: `cargo test -p bri-sim` (all pass; new
`a_wrench_send_replaces_a_faded_item_with_a_fresh_one` and a restock check in
`items.rs`); `cargo test -p bri-client --release --test world_items --
--ignored` (8 pass, new `a_picked_up_brick_item_stays_as_a_ghost_until_it_respawns`);
new `crates/client/tests/item_ghost.rs`, through the real App headless:
plant a 2x2 brick, wrench a gun onto its side, pick it up by contact, see
alpha 0.25 on every client and capture it in first and third person; the
wrench's Send then restocks it solid; pick it up again under the ordinary
8 s, stand in the ghost, and take nothing until the respawn tick, when it is
taken at once. Single player and LAN (guest's pickup seen by the host, then
the host's seen by the guest) both pass (`--ignored --test-threads=1`, 81 s).
Frames in `artifacts/item-ghost/`. The test closes the LAN host's firewall
question unanswered. Not seen in a window: Max's playtest.
## 2026-09-28 Screens driven through to the server (branch `claude/bug-sweep-ui-harness`)

Why: Max's recent bugs came from screens reading the wrong widget (Avatar
Done read the label, brick search read the wrong box) while tests called the
server directly or used synthetic layouts, and from tests only as the host.

Two harnesses, both driving only what a player does (clicks at a control's
centre after checking the click reaches it, typed characters, keys, wheel):

- `crates/ui/tests/field_flow.rs` (fast, no server): opens each converted
  v20 screen (`content/ui-pack-004`) as the game does, changes each visible
  text box, checkbox, radio button and dropdown on its own, presses the
  screen's button, and diffs what it emitted (`UiAction`s and settings, as
  JSON) against an unchanged run. The changed paths must be exactly the ones
  in the screen's table, and a typed value must be the value found there.
  List screens check that the row clicked is the one acted on. Controls not
  sent are listed with a reason; a control no table names fails the test.
  Covers Start Game (single player and LAN), Advanced Config, Join Server,
  Connect to IP, Avatar, Choose Name, Save/Load Bricks, Brick Selector
  search, Player List, Join/Create Mini-Game, all three wrench dialogs, the
  events editor (plus a row built by clicks), the Copy boxes, admin login,
  Kick, Ban, Un-Ban, Change Map, Host options, admin passwords and server
  identity, chat and console. Reverting the Avatar name fix or the admin
  login fix below makes it fail on exactly that field.
- `crates/client/tests/screen_topologies.rs` (ignored, content-backed, ~80 s
  debug): three hosted Bedroom games on a free test port — single player, a
  LAN host with a joined guest, an internet host with a guest who starts with
  no trust — each driven through Start Game (with Advanced Config), Avatar,
  Connect to IP, Brick Selector, wrench and events dialogs, Save/Load Bricks,
  Admin brick list Clear All, Player List trust, Create/Join Mini-Game, admin
  login, Host options, chat and console, asserting what the server did (45
  checks). It declines the Windows Firewall question instead of opening it.

Bugs found and fixed:
- The admin password box ignored Enter, its only way to log in (the layout
  has no button). Enter in a text box now runs its `altCommand`, as Torque's
  GuiTextEditCtrl does (`screens::event_command`).
- Start Game forgot the typed server name and passwords. Text boxes bound to
  a preference now write it as they are typed in, as Torque's `variable`
  does, so `$Pref::Server::Name` and the passwords are kept like v20 (v20
  kept them in prefs.cs too).
- A click that turned and fired in the same frame swung along the old
  facing: the trigger's own aim (`ActionAim`) was used only on the tick the
  trigger was read, but the wrench swings two ticks later (PreFire) and the
  movement carrying the turn could arrive after that. v20 sent the trigger in
  the same move as the look. The host now keeps an aimed click's direction
  until that click's shot (`ToolFire` or a spawned projectile), for at most
  60 ticks; clicks without an aim still use the body's facing when they fire.
  Test: `bri-sim` `tools::a_click_lands_its_delayed_swing_where_it_aimed`.
- A joined guest's game showed the typed address as the server's name and 64
  as its size (Player List "127.0.0.1:28000 - 2/64 Players"). The handshake
  listing now names the joined server (`network::View::listing`,
  `app::joined_server`). Test: `a_joined_server_goes_by_its_listed_name_and_size`.

Checked and matching v20, so kept: LAN and single player trust everyone
(`getTrustLevel` returns You when `$Server::LAN`); a click during the
wrench's half-second swing does nothing; a changed event input or target
clears the rest of the row, which is not sent until an output is picked
(`createTargetList`, `wrenchEventsDlg::send`); any player may save the
bricks they see, and only an administrator may load (`SaveBricks_Save`,
`serverCmdInitUploadHandshake`).

Reported to other lanes, not changed here (name fix): the server ignores
the Avatar screen's clan prefix and suffix (v20 `onConnectRequest` keeps 4
characters of each and chat shows them), and v20 cut LAN names at 23
characters where ours keeps 48; a fresh install asks for a name twice (the
`regNameGui` prompt at startup and the name message box after the first-run
welcome).

Evidence: `cargo test -p bri-ui` (all pass, field_flow 3 tests),
`cargo test -p bri-sim` (all 37 targets pass), `cargo test -p bri-client
--test screen_topologies -- --ignored` (1 passed, 45 checks), `cargo clippy
-p bri-sim -p bri-client -p bri-ui --all-targets -- -D warnings`. Not seen in
a window: Max's playtest of admin login by Enter, a guest's Player List title
and a quick turn-and-wrench on a hosted game.
## 2026-09-28 Default Add-Ons come with the repository (branch `claude/project-thread-k1na0c`)

Max: the Stunt Plane and the Duplicator should come with the repo, so a
from-source run has them with no extra step. Before, the Duplicator lived in
`packages/duplicator` but only the release packager copied it into
`content/addons` and listed it; the Stunt Plane was imported at build time
from an archive only Maxwell's PC has, so no other checkout could get it.

Design picked: the first run installs the defaults, rather than loading them
in place. Package directories must stay inside the content root (a safety
check, and content identity names them root-relative), so loading from
`packages/` would have meant loosening both.
- `packages/default-addons.json` is the one list, in load order:
  `duplicator`, `duplicator-tool`, `vehicle_stunt_plane`. The runtime
  (`bri_package::defaults`, compiled in), the packager, `-VerifyPackage`, the
  packaging test and `tools/default_addons.py` all read it.
- The Stunt Plane is committed in converted form at
  `packages/imported/vehicle_stunt_plane`, imported once from
  `Vehicle_Stunt_Plane.zip` (sha256 `e68fd173…329e`) by the existing importer
  (`python tools/default_addons.py import`). Its assets are byte-identical
  to the old `content/shipped-addons` copy; the report now names the archive
  instead of a path on Maxwell's PC. `tools/shipped-addons.json`,
  `tools/shipped_addons.py`, bootstrap's import step and the copy in
  `ci-content.zip` are gone.
- Running `bri-client` and `bri-server` install them when the content root
  sits in a checkout (`content/../packages/default-addons.json`
  exists): each is copied to `content/addons/<id>` when missing or different
  from the checkout's copy, built beside the target and swapped in. A
  release's content is never touched.
- No `packages.json` is written. Without one, `PackageSet::load_root` loads
  the base list followed by the installed defaults, exactly what a release's
  `packages.json` lists, so a checkout keeps following `base-packages.json`.
  When the player has their own `packages.json`, a default it neither turns
  on nor off is turned on, one they turned off stays off, and listed entries
  follow the installed copy's version.
- The Add-Ons screen's Default button keeps the default Add-Ons on ("Keep
  only the base game and the default Add-Ons?").
- The showcase Add-Ons stay out of releases and of the defaults.

Evidence: `cargo test -p bri-package` (30 passed; new `defaults` tests: the
list is whole with its dependencies met, a fresh root gets all three with no
list written, a changed copy is replaced, a player's own list keeps what they
turned off, only a checkout's content is installed into);
`cargo test -p bri-client --lib add_ons`; `tools/tests/Test-PlaytestPackaging.ps1`
passed (the release lists the three after the base game as server, shared,
shared; verify refuses a release without the plane; the packager refuses to
build when the plane is missing from `packages/`). New
`crates/client/tests/default_add_ons.rs` passed with the generated content:
a temporary checkout of `packages/` and a `content/` of only the base packs;
`bri-client --check` installs all three and writes no list; in single player
`/dup` gives the Duplicator and a loaded Stunt Plane spawn brick spawns the
plane; in a LAN game a guest from the same checkout joins with nothing
downloaded, `/dup` gives them the Duplicator, and the plane shows on their
screen and in their vehicle list. `add_on_join`
`a_guest_without_the_stunt_plane_downloads_it_and_can_spawn_it` no longer
assumes `content/shipped-addons`: the host uses the content's own copy or
stages the repository's, and the guest turns the plane off explicitly. It
passed on a base-only content root and on one with the defaults at
`addons/`. Not seen in a window: Max's playtest from a fresh checkout.
Known limit (existing behaviour): a `packages.json` the Add-Ons screen writes
in a checkout pins the base list as it was then.
## 2026-09-28 Local body and held item no longer shake while looking around (branch `claude/project-thread-j5cwjx`)

Max: turning the view, the camera was smooth but his own body (third person)
and whatever he held (first person) shook slightly; vanilla v20 is smooth.
Cause: a frame was sampled twice. The window loop ticks in `about_to_wait`
(prediction, `Motion::present` with the mouse yaw, avatar pose, first-person
image placement from the Eye node and view angles) and then requests a
redraw. Raw mouse motion that Windows delivered while the tick ran is
dispatched to `Controls` before `RedrawRequested`, and `render_scene` drew the
camera from the live `Controls`. The camera therefore turned by motion the
body and images never saw, a different amount each frame (the split depends
on where the motion lands against the tick). v20 draws the first-person
camera and the mounted images from one eye transform
(`Player::getRenderEyeTransform`, 0x5aafa0) per frame, so nothing can drift.

Fix: `App::tick` snapshots the `Controls` it posed the frame with
(`drawn_controls`), and `render_scene` draws the camera, FOV, view mode and
name-tag observer check from that snapshot. Later motion shows next frame,
like everything else: the added delay is only for motion that arrived during
the tick, which the next tick consumes, so input-to-photon latency is the
same as running the tick inside the redraw. Prediction, render
interpolation (fixed ticks blended by the accumulator), the render-rate
yaw and smoothed reconciliation were already in place and are unchanged.

Evidence: new headless probe `cargo test -p bri-client --test view_jitter
--release -- --ignored --nocapture` (Bedroom, single player over loopback
QUIC, offscreen GPU, no window). It turns at 2.5 rad/s over 300 uneven frames
(3 to 14 ms sleeps plus a 25 ms hitch every 37 frames; measured 4 to 116 ms
with render cost), splitting each frame's mouse motion at a random point
before and after the tick, holding the hammer. It tracks the held image, the
body origin and the right hand (Mount0) in the camera's frame; "jitter" is
the distance from where the point's previous velocity carried it.

| case | held item jitter rms (before -> after) | body yaw vs camera max |
| --- | --- | --- |
| 1st person, standing | 39.8 mm -> 0.015 mm | 3.5 deg -> 0 |
| 1st person, walking | 84.2 mm -> 0.015 mm | 14.9 deg -> 0 |
| 3rd person, standing | 52.0 mm -> 0.017 mm | 6.8 deg -> 0 |
| 3rd person, walking | 28.3 mm -> 30.0 mm (run-cycle arm swing) | 3.6 deg -> 0 |

Body origin jitter is 0.009 mm rms in every case after the fix (4 to 8.6 mm
before in first person). Third person walking: the item rides the swinging
hand of the run clip, which is keyframed animation, identical before and
after and independent of the camera; the probe asserts yaw and body origin
there, and item stability everywhere else. `cargo test -p bri-client --lib
--release` 195 passed; `cargo clippy -p bri-client --release --tests` clean.
Defaults picked: the snapshot, not moving the tick into `RedrawRequested`
(same latency, no event-loop restructuring); LAN guest and host use the same
local-player path as single player (loopback QUIC), so the probe covers them
by construction, not by a second session. Not seen in a window: Max's
playtest turning in first and third person at 144 Hz with VSync on and off.

## 2026-09-28 Hammered and wanded bricks fall through the world (branch `claude/tool-kill-feel`)

Max: tool-broken bricks stayed as colliding debris that took a while to get
out of the way; in v20 they fell through the ground and faded quickly,
unlike bricks knocked out in a minigame. `blocklandv20.exe` confirms two
paths (details and addresses in
[audits/brick-damage.md](audits/brick-damage.md#how-a-dying-brick-looks-2026-09-28-branch-claudetool-kill-feel)):
`killBrick` throws the brick up at 8 units/s, spins it, lets it fall with
no collision (16 t^2) and fades it after 0.5 s at rate 3/s; brick
explosions are physics bodies. The `BrickKill` cue now carries
`BrickDeath::{Kill, Blast}`, chosen on the server by cause, and
`BrickDebris` draws kills as closed-form falling bricks (no Rapier body,
the same on every client and frame rate, independent of Physics Quality as
in v20) and blasts as before. Wire change: `CueKind::BrickKill` gained a
field, so old and new builds must refuse each other (protocol number left
for Gate). Debris model instance capacity is now `3 * MAX_LIMIT` so bodies,
ghosts and falling bricks of one look fit.

Measured headless, host and joiner alike: hammer/wand bricks were solid
4.98 s and within 2 units of their spot 4.88 s; now never solid, clear in
0.55 s, invisible (alpha 0.05) at 1.48 s. Minigame rocket debris unchanged.
Evidence: `cargo test -p bri-client --test tool_kill_feel -- --ignored
--nocapture`, `cargo test -p bri-client --lib -- brick_debris audio
--include-ignored` (16 passed; new `a_tool_kill_hops_spins_and_falls_through_everything_as_it_fades`,
`tool_kills_fall_the_same_on_every_client_and_frame_rate_whatever_the_limit`),
`cargo test -p bri-sim --test tools --test brick_damage -- --include-ignored`,
`--test blocks --test packages --test hardening_packages --test
hardening_session --test session --test duplicator`, clippy on bri-sim and
bri-client all targets. Not seen in a window: Max's playtest.

Left for the entity-perf lane (not changed here): with Physics Quality Off,
v20 still draws blasted bricks falling ballistically, while ours throws
nothing; v20 evicts old physics bricks into a ballistic fall that fades
after 0-0.5 s (0x5338c0) where ours drifts linearly for 0.35 s.

## 2026-09-29 Stunt Plane contrails; its steering is v20's (branch `claude/project-thread-bobr1s`)

Max's a22 playtest: only the plane's left wheel turns with the mouse, and
the streams off the wing tips at speed are missing.

Steering is unchanged; it is v20's. `WheeledVehicleData::onAdd` (recovered
core scripts; v1 and v21 alike) steers only wheel 0 of a 3-wheeled vehicle,
and the plane's `onadd` calls `Parent::onAdd`. Its `hub0` is the front-left
wheel (`hub2` is the tail wheel). blocklandv20.exe draws each wheel turned by
`mSteering.x` times its own steering (0x571e34: `[obj+0x82c] * [wheel+0x4c]`,
the field `setWheelSteering` writes at 0x571526), so in v20 too only that
wheel turns. The import test now says so instead of "nose wheel".

Contrails, as stuntplane_Contrail.cs does them: `contrailCheck`, started by
`onadd`, mounts `contrailImage1`/`2` in slots 2/3 while
`vectorLen(%obj.getVelocity()) >= minContrailSpeed` (30). The images mount
at `mount3`/`mount4`, the wing tips (x = ±4.5), and their `FireA` state runs
`ContrailEmitter` for 10000 s: a particle per millisecond, no velocity,
0.5 s life, white to clear blue, the base game's cloud texture.

- Vehicle schema (still 6; optional fields): `trails` (node, emitter frame,
  emitter id, speed range) and `effects` (the vehicle's own particles and
  emitters in the effects library format, validated as the library does).
- Import Add-On: `vehicle_script` reads `mountImage` in `onAdd` and the
  functions it calls, with the speed test's range; a threshold may be a
  datablock field (`%obj.dataBlock.x`, `%this.x`). An image whose state holds
  an emitter (60 s or more, or re-entering itself) becomes a trail at
  `mount<mountPoint>`, placed by the image's offset and rotation
  (`bri_weapons_import::image_placement`; `bri_weapons::rotation::native`,
  moved from the client). The Add-On's emitter and particles convert with
  `bri_convert::effects` under its namespace; a particle drawing an
  Add-On texture is reported unsupported. The two contrail images are no
  longer reported unsupported.
- Client: `with_vehicle_effects` adds the vehicles' particles and emitters to
  the actor effects pack; each frame `vehicle_trails` runs a trail's emitter
  at its node while the presented speed is in range, through ActorEffects'
  sources and the effects world's budgets. Cosmetic, from the replicated
  vehicle pose; no protocol change.
- `packages/imported/vehicle_stunt_plane` regenerated with
  `tools/default_addons.py import --only vehicle_stunt_plane` (v20 reference
  on E:); the diff is only the trails, effects and report entries.

Default picked: a trail starts and stops the frame the speed crosses 30,
where v20's script checks every 2 s (as the speed-switched propeller does).

Evidence: `cargo test -p bri-addon-import` (new `vehicle_script` test;
`real_community_samples` imports the real plane: trails at mount3/mount4 from
30, converted emitter and particle, images consumed);
`cargo test -p bri-client --test vehicle_trails` (the plane flown at full
throttle in the vehicles runtime, seen by its driver and by a guest whose
pose went through the datagram codec: no trail below 30, both tips from 30,
800 to 1100 particles in lines 4.5 either side behind the plane; below 30
they drain to none; `--ignored`: the base effects pack has the cloud
texture); `cargo test -p bri-vehicles -p bri-weapons -p bri-weapons-import
-p bri-vehicles-import`, `cargo test -p bri-client --lib --test
actor_effects --test tire_spray`, `add_on_join -- --ignored
a_guest_without_the_stunt_plane_downloads_it_and_can_spawn_it`. Not seen in
a window: Max's playtest.
## 2026-09-28 Bug-pattern sweep, cloud part (branch `claude/bug-pattern-sweep-h0v7ns`)

Max asked for the common pattern behind his recent reports and how to catch
the next ones first. The five patterns, the checks that now enforce them and
the hard-stop audit are in `docs/audits/bug-patterns.md`. In short: screens
and the server disagreeing, guessed v20 rules locked in by tests, testing
only as the host, hard stops instead of fallbacks, and missing budgets.

Fixed: UI requests have deadlines and screens can back out while sending;
client background jobs, network errors and a broken Add-On no longer close
the game (the Add-On is left out with a message); a panicking host request
or step is answered and logged and the host keeps going (fuse: 8 a minute,
then stop with autosave); map load and per-player placement failures are
reported instead of failing the host or the map change; weapon updates over
the wire limits carry over; four overflow or unwrap panics. Event jobs now
share their compiled row, action and output, so an administrator's
zero-delay loop costs about 6 ms a tick instead of 24 ms (release, fuzzer's
worst seed), and the host logs event notes, rate-limited.

Evidence: new `command_fuzz` (every `Command` variant, damaged, as guest and
host; 256 cases) and `event_fuzz` (random catalog programs, 32 ms tick
budget in release) in `bri-chaos`; unit tests for the UI deadline, wrench
Escape, the panic fuse and the weapon clamp; `cargo test` and clippy
`-D warnings` on bri-ui, bri-net, bri-sim, bri-events, bri-package-runtime,
bri-client and bri-chaos. Content-backed test
`a_broken_add_on_is_left_out_instead_of_stopping_the_game` runs only on the
PC gate. Routed to other lanes: name length refusal, all-or-nothing Load
Bricks, poisoned admin store. Next: the PC part (real-screen harness as
single player, host and guest; v20 behaviour audit).
## 2026-09-29 Kitchen palms and map-model foliage (branch `claude/project-thread-q34j2j`)

Max saw Kitchen palms and shrubs drawn as sparse comb stripes, missing
parts depending on the view, with the far palm washed out. Causes, checked
against Blockland's TGE engine source (`TSMesh::initMaterials` culls DTS
back faces; `TSMesh::setMaterial` enables blending and turns depth writes off
only for Translucent materials; nothing enables alpha test):

- Opaque DTS materials were alpha-tested at zero. The Sharp_Trees frond
  stems (palm material 0, 288 triangles per crown) sample a texture strip
  whose alpha is 0, so every stem vanished and the leaflet combs floated
  detached. Materials now carry `ignore_texture_alpha`, set for every
  non-Translucent map-model material; the shader flag shares `material[0].w`
  with the temp-brick flash as bit 2.
- Alpha-tested images used plainly averaged mips, thinning leaves until only
  the blended soft edges remained at distance. Upload now builds
  `chain_preserving_coverage` for any image a `Mask` material samples.
- Culling stays one-sided: the leaves are authored with reversed duplicate
  faces (verified per triangle), matching Torque. Transparent texels in the
  tree sheets are already leaf-coloured (1-4% near-white), so no colour
  bleeding pass is needed.

Evidence: before/after offscreen captures of Kitchen palms and Bedroom
trees with `scene_snapshot`; `real_native_maps_upload_once_camera_motion`
asserts opaque tree materials ignore texture alpha; new ignored
`map_shapes::kitchen_palms_render_for_host_and_guest` hosts Kitchen on LAN,
joins a guest over loopback and renders both players' frames. No protocol
or content change.
## 2026-09-28 v20 behaviour audit (branch `claude/bug-sweep-v20-behaviour`)

Why: rules guessed and then locked in by tests (the indestructible spawn
brick blocking the hammer) kept reaching playtests. `docs/audits/v20-behaviour.md`
compares every event input and output, every `serverCmd*` and the script
brick datablock flags with v20's server scripts, rule by rule, with a
v20 file:line, our file:line and a status.

Fixed, one test each citing the v20 line (`crates/sim/tests/v20_events.rs`,
`hardening_session.rs`, `vehicles.rs`):
- Kill bricks and the other Player/Client outputs act on whoever set the
  input off, outside minigames too (`Player::kill` has no minigame check);
  the old `harmful()` gate was a guess. Hurting outputs still respect the
  2.5 s spawn protection of `Armor::Damage`.
- Single-player/LAN servers give the MiniGame target from the activator's
  game, and a game's owner may Reset it from any brick.
- spawnItem/Projectile/Explosion do nothing from fake-killed or hidden
  bricks; radiusImpulse pushes only the activator on internet servers
  outside minigames; recoverVehicle leaves a ridden vehicle alone.
- `/cancelEvents` works for players (5 s, not in another's minigame, admins
  only on LAN). v20's 100-row and 30 s delay limits are deliberately not
  copied: the alpha contract keeps 1024 rows and 300 s delays.
- Holding the admin wand skips touch events.

Kept different, with reasons in the audit: touch immunity (our touches fire
once on contact), instant `/suicide`, relays limited to the owner's bricks
(v20's check was always false), delayed Projectile rows. Open: onBotTouch's
Client/Driver targets, radiusImpulse on vehicles/items/corpses, "Do not
repeat yourself", chat URL links, fakeKillBrick with 0 s, `/tripOut`.

Evidence: `cargo test -p bri-sim --no-fail-fast` (all 39 targets pass) and `cargo clippy -p bri-sim
--all-targets -- -D warnings` once at hand-off, per Max's rule to compile
less. Not seen in a window: a kill brick in free build on a hosted game.

Round 2 (same branch): names are cleaned instead of refused; a damaged
admin store is set aside and the host starts; an unconfirmed admin save no
longer stops the host; each owner's events get a per-tick share (counted
in cost units since round 4); the client network
worker fails a single slow request instead of disconnecting. Evidence:
`joins_take_a_cleaned_name_instead_of_being_refused`,
`missing_store_is_initialized_and_a_damaged_one_is_set_aside`,
`one_owners_zero_delay_loop_stops_at_its_share_and_others_still_run`,
`a_time_budget_stops_slow_rows_and_keeps_their_order_for_the_next_phase`,
`a_slow_or_lost_answer_costs_its_request_not_the_connection`; event fuzzer
48 cases in release pass the 32 ms tick budget.

Round 3 (same branch): clan tags and the double name prompt. The Avatar
screen's clan prefix and suffix now join with the name (`Hello::clan`) and
change on Avatar Done while connected (`Command::SetClan`). The host cleans
them as v20's `GameConnection::onConnectRequest` (mainServer.cs 1631-1664)
does, `trim(getSubStr(StripMLControlChars(%clanPrefix), 0, 4))`, and the
Avatar boxes keep v20's `maxLength = 4`. Names now follow the same rule with
23 characters (was 48, which came from the first playtest prep, not from
v20); duplicate numbering stays within 23. `StripMLControlChars` is a small
local strip (`clean_connect_text`) until the shared Torque ML parser lands.
Only chat and team chat read the tags (mainServer.cs 1098 and 1176,
`'\c7%1\c3%2\c7%3\c6: %4'`): grey tags around the yellow name; kill
messages, name tags and the player list keep `getPlayerName()` alone.
Protocol bump (Gate renumbers). A fresh install asked for its name twice (a
"Your Name" message, then Choose Name); `Core::name_prompt` now opens Choose
Name once per run and the message is gone. Evidence:
`clan_tags_are_cleaned_and_carried_on_chat_lines`,
`joins_take_a_cleaned_name_instead_of_being_refused` (sim),
`clan_tags_from_the_join_and_avatar_done_reach_chat` (net, host and guest
over QUIC), `chat_lines_carry_v20_colors` (client),
`first_run_offers_the_tutorial_then_asks_for_a_name_once` (UI screens);
content-backed on the PC gate: `avatar_clan_tags_show_in_chat_as_single_player_and_guest`
and `first_open_asks_for_a_name_once`.

Round 4 (same branch): event budgets are counted, not timed. Gate found
Pong's own rows deferred on a busy PC in a debug build, because round 2's
per-owner budget was wall-clock time, so the game played differently by
machine speed. The engine's `TimeBudget` and `advance_within` are gone;
`Limits::cost_per_scope` and `cost_per_phase` budget each phase in cost
units (a row costs 1 plus each job it expands into). The host sets 4000
units per owner and 8000 per tick (`bri_sim::session::event_limits`),
calibrated with `BRI_BENCH=1` on the event fuzzer in release: 0.6 to 1.5 us
per unit on the loop-heavy seeds (median about 1 us), so about 4 ms per
owner. Wall time is only a watchdog: event phases over 8 ms are counted
(`Session::take_slow_event_ticks`) and the host logs one line per 10 s; it
never changes which rows run. The fuzzer checks the engine's counts each
tick and that the same programs leave the same bricks and queue twice;
`the_budget_runs_the_same_rows_on_a_slow_machine` runs one queue on a fast
and a 1 ms-per-row host and gets identical phases. Tests pass with every
core busy (Pong needs content; the PC gate runs it).

Round 5 (same branch): the five v20 event gaps from the PC audit and the
last wall-clock test. onBotTouch now fills Driver (seat 0) and Client (the
spawn brick owner, else the driver, else on LAN the first player; with none
the rows don't run). radiusImpulse divides by mass (players and corpses 90)
and on LAN or in a minigame also pushes vehicles and dropped items, filtered
by the game's damage rules; item mass 1 is inferred. fakeKillBrick 0 s
restores on the next tick. `/tripOut` (administrators, silent) gives every
brick Undulo and the rainbow colour effect in one collision pass
(`Simulation::mutate_many`). A chat line repeated within 15 s warns "Do not
repeat yourself." and uses up the second's chat allowance. The sandbox
shader loop test checks counted loop limits; its timings need `BRI_BENCH=1`.
Avatar choices the host lacks fall back to defaults instead of refusing the
whole change. Evidence: `crates/sim/tests/v20_events.rs` (10 pass; the item
case needs content), `unknown_avatar_choices_fall_back_to_defaults_and_keep_the_rest`.
Open: the net loopback 1024-row event test fails since the v20 behaviour
merge, which truncates rows to 100 (see `docs/audits/bug-patterns.md`).

Gate follow-up (same day): the first `item_ghost` failed on the loaded gate
PC ("respawned before the ghost was captured"): walking away and capturing
took longer than the 8 s respawn. The hosted server runs on the wall clock
(`net/src/server.rs` ticker), so the test no longer races it. The first
ghost is held by v20's longest respawn (`$Game::Item::MaxRespawnTime`,
300 s) while it is inspected, and the wrench's Send restocks it, which also
covers `fxDTSBrick::setItem` end to end. The ordinary 8 s respawn is judged
in sim ticks: available 960 ticks after the pickup, taken again at that tick
(single player 2604/2604; LAN 1819/1824 and 4238/4242), 7.94-8.02 s of wall
time, so the real game still respawns on time. The test also waits for the
third-person camera slide to end and for the server to hold the aim before a
wrench swing; both had made swings miss.
## 2026-09-28: Gate and build speed (first cut)

sccache was already on for every cargo run on this PC through
`~/.cargo/config.toml`, but the gate never hit the cache the lanes fill.
sccache hashes every `CARGO_*` variable, and the gate set `CARGO_TARGET_DIR`.
Probe on a private sccache server, building `bri-content` twice: two target
dirs set by the environment variable gave 0 of 11 hits, and the same dirs
passed as `--target-dir` gave 8 of 11 hits, including across two worktrees.
The misses were the workspace crate and build-script crates (`CARGO_MANIFEST_DIR`
and `OUT_DIR` differ). `SCCACHE_BASEDIRS` did not change this. The gate now
passes `--target-dir`. Lanes must not set `CARGO_TARGET_DIR` either.

The gate's test step took 230 to 600 s, and its long pole was
`bri-chaos/session_chaos`: one ignored test ran two maps times four seeds in
series, about 290 s. It is now eight tests (a map slot by a seed shard), which
libtest runs in parallel. Together they cover every map and seed exactly once,
and a unit test checks that. A retry of a new failure now reruns only the
binary it came from, not `cargo test --workspace` behind a name filter
(16 to 96 s each). The gate log now lists every test binary's run time.

Next: a cold versus warm lane build, cargo-nextest against the gate's own
runner, and rust-lld for the roughly 235 test binaries the gate links.

## 2026-09-28 Large builds: frame time (branch `kitchen-perf`)

Max: Badspot's Birth Day on Kitchen (47,061 bricks, 64 lights, 266
emitters) ran at about 25 fps on his RTX 4070 SUPER at 1440p, and Badspot's
Block Party Christmas 09 on Slate (75,155 bricks, 457 emitters) lagged far
worse. Reproduced headless with the new
`cargo test -p bri-client --release --test large_build_perf -- --ignored`
(BRI_PERF_SAVE, BRI_PERF_SETTINGS=Max's settings.json, 2560x1440, DX12):
the normal App hosts the save's map, loads the save as a dropped `.bls`
converts, and renders three views offscreen. Times are the game's own:
update and recording spans as the platform loop measures them and
`GpuFrameTimer` timestamps; `RenderStats` counts draws and binds, which do
not move with machine load. An in-process sampling profiler
(`tests/support/sampler.rs`, no admin rights needed) writes folded stacks.

Root causes and fixes:
- Effect line of sight (`Building::effect_visible`, every emitter and flare
  every frame) collected every brick in the box around the whole sight line
  into a BTreeSet and ray-tested each. On Slate this was 40% of the frame
  and up to 190 ms of update. It now walks the grid buckets the ray
  pierces (the DDA the server's raycast already uses), slab-tests bounds
  and stops at the first blocker.
- Chunks drew once per brick surface image (5 surfaces plus prints and
  blended copies), and translucent bricks once per surface: 11,903 draw
  commands at Kitchen spawn, with pipeline, material and buffers rebound
  for each. Now one `BrickSurfaces` material binds all five surface images
  plus white; each vertex names its slot and the shader samples exactly as
  the separate materials did. Chunk geometry lives in shared pooled blocks
  and runs of batches that bind the same things draw with one
  `multi_draw_indexed_indirect`, in the world pass and in each shadow
  cascade. Repeated binds are skipped; chunks draw nearest first.
- Liquids were cloned (names, images and all) three times a frame; they are
  now cached per collision-mirror water generation and palette.

Measured, medians of three interleaved runs per build on a loaded machine
(other lanes compiling), p50 ms:

| Save / view | before frame (update, record) | after frame (update, record) | draw commands |
|---|---|---|---|
| Kitchen spawn | 86.0 (18.0, 39.5) | 46.1 (6.1, 19.1) | 11,903 to 2,071 |
| Slate spawn | 108.7 (54.8, 36.5) | 61.7 (8.8, 31.6) | 6,200 to 980 |
| Slate overview | 318.6 (194.3, 68.0) | 84.9 (9.7, 54.0) | 12,247 to 2,221 |

Frames match the old renderer pixel for pixel within run-to-run noise
(largest: Slate inside 1.56% of pixels differ, two old-renderer runs differ
1.99%, from particles). Remaining costs on these saves, largest first:
particle sorting in `EffectsWorld::snapshot` (21% of Slate's frame; the
entity-perf lane owns emitters), wgpu pass encoding, the effects renderer.
144 fps is not reached yet.

Technique checklist for brick rendering:
- Done: redundant bind elimination, front-to-back opaque order, one
  material for brick surfaces, pooled chunk buffers with indirect
  multi-draws (world and shadows), DDA sight lines, per-frame clone removal.
- Already present: chunk frustum culling, per-cascade shadow culling,
  incremental chunk rebuilds, multithreaded chunk meshing.
- Next: v20 COVERAGE face culling in chunk meshing, compact chunk vertices,
  fewer translucent draw runs, then evaluate occlusion culling, clustered
  point lights and cached static shadow cascades against the profiles.
- Skipped: LOD and impostors (fog bounds the view, and they would change
  v20's look).
## 2026-09-29 Entity performance: emitters, vehicles, weapons, debris (branch `claude/entity-perf`)

Max: a big session lags from more than bricks. `entity_probe`
(`cargo run --release -p bri-client --bin entity_probe -- content out [scene]`)
builds idle, emitters (480 emitter bricks, 64 lights), vehicles (64, 48
driven), weapons (32 players firing guns and rocket launchers at a wall),
blast (16 launchers into a 4,000-brick pile) and water (64 water bricks) on
Slate; steps the host at 120 Hz and serves each scene on loopback to a real
`App` guest rendering offscreen at 1920x1080. It reports this thread's CPU
cycles and allocations per stage (other lanes share the PC, so wall time and
fps are noise), `BRI_PROBE_PROFILE=1` adds a sampling profile (x64 unwind +
PDB names, no admin rights), `BRI_PROBE_REMOVE=100000,1000000` times brick
removal in big worlds.

Measured (Mcycles; host per tick, client per frame; before -> after):
- Host, 64 vehicles / 48 drivers: 509 -> 2.4 (136 ms -> 0.6 ms; weapon damage
  checks built every vehicle's snapshot per player per vehicle).
- Host, 64 water bricks: 0.25 -> 0.045 (liquids shared, not copied).
- Host, removing one brick: 6.0 -> 0.02 at 100k bricks, 90 -> 0.08 at 1M (the
  broad phase refit the whole tree per removal; colliders now park).
- Client, 480 emitters: 14,400 -> 79 particle draw calls, 77 -> 29.
- Client, 50 players on vehicles: 58.6 -> 25.0; 32 firing: 53.9 -> 31.6;
  blast: 58.9 -> 31.5.

Standard techniques, status: batching (particles one texture array and one
premultiplied blend; brick removals, collapses and blasts) done; instancing
(bodies through a per-body transform; held items share still poses) done;
pooling/parking (removed colliders) done; view culling (particles, bodies
without shadows) done; spatial prefilter (vehicle contacts by bounds) done;
lazy work (damage policy per hit, meshes built at render) done; caching
(liquids, node and definition indices, resolved effect names) done; radix
depth sort done. Not yet: animation rate LOD at distance, per-instance
culling of vehicles and items, GPU skinning, moving work off the main thread.

Evidence: 421 bri-sim/vehicles/fx-runtime/render and 250 bri-client tests
(lib plus avatar, crouch, movement, world item and item rendering suites,
ignored included) pass; clippy `-D warnings` on those crates. New tests:
reposed bodies equal a full rebuild bit for bit; still item sequences equal
the rest pose; radix order equals the stable sort; in-view culling; frustum.

## Brick loading and joining speed (claude/brick-load)

Measured with `cargo run --release -p bri-client --bin brick_load_bench --
content <bench-dir> report.json [runs] [synthetic-bricks]` on Badspot's Birth
Day (Kitchen, 47,061 bricks), Badspot's Block Party Christmas 09 (Slate,
75,155) and a synthetic million-brick world, on a PC at 100% CPU from other
lanes (CPU times and bytes are the steadier figures).

| | Before | After |
|---|---|---|
| Birthday load done for every player | 8.4 s | 0.6 s |
| Birthday upload to the host | 20.2 MB | 0.88 MB |
| Birthday join playable / download | 0.28 s / 590 KB | 0.11 s / 340 KB |
| 1M Load Bricks | refused (over 64 MB) | 1.1 s CPU read + 7.5 s to every player |
| 1M join playable | 6.8 s (whole world first) | 0.9 s (45k nearby bricks first) |
| 1M save file | 463 MB JSON | 5.3 MB packed |
| Idle tick after loading 1M | 7 ms | 0.01 ms |

Decisions: no fixed load pacing (a 7 ms per-tick budget; tests pin
`LoadPace::Bricks`); per-system change readers (`session/dirty.rs`);
protocol 54 packs bricks (`bri_world::packed`) and compresses bulk requests;
world transfers go nearest neighbourhood first and joiners play once bricks
within 64 units (at most 50,000) arrive; saves are packed binary behind a
header, JSON saves still load; a bad save line is skipped like v20's
`ServerLoadSaveFile_Tick`. session_chaos checks each changed brick once.

Next: placement is now mostly Rapier inserting static colliders (about 60%);
compound colliders per chunk would be the next step (physics lane). Mesh
building for a 1M world is the render lane's.

### 2026-09-29 Large builds, second increment (branch `kitchen-perf`)

- v20 COVERAGE face culling in chunk meshing (`crates/client/src/brick_cover.rs`):
  a face is left out when opaque, visible, undisplaced neighbours whose
  touching face hides adjacent cover its required area. Triangles drawn:
  Golden Gate 46% of every face, Christmas 09 39% at spawn. perf_probe's
  chunked-versus-whole-world check: 0.000% of pixels differ on Pirate World,
  at most 0.022% on Golden Gate.
- Ghost brick cached per look and placed by a transform: 18.3 ms per
  change before (rebuild with textures), 0.2 ms per move now; world changes
  elsewhere no longer rebuild it.
- Chunk pool: loads reserve one block, blocks grow geometrically, and
  translucent batches live in their own pool, so back-to-front sorting keeps
  long indirect runs. Golden Gate records 44 world draws and at most 4
  shadow draws; the million-brick city 27 (from 20,370).
- The client's brick budget now counts drawn triangles (16M, after culling);
  4M before culling disconnected a player at about 200k simple bricks.
- `Building::trace` (name tags each frame, tool targeting) walks grid
  buckets along the ray; it was 68% of a frame over the million-brick city.
- Gate check: `cargo test -p bri-client --test brick_draw_budget -- --ignored`
  asserts draw counts and the culled share on the largest stock save.
  Counts only, no timings.

Benchmark (`large_build_perf`, one run each, 1440p, Max's settings,
machine shared with other lanes): see the hand-off reply for the final
numbers. Synthetic city: `BRI_PERF_SYNTHETIC=<bricks>` builds hollow
1x1-brick towers on Slate and loads them in 50k-brick parts.

Left for other owners: particle sorting in `EffectsWorld::snapshot` is the
largest remaining CPU cost on the emitter-heavy saves (entity-perf lane).
Crate tests: bri-render, bri-sim lib, bri-client lib (201 passed),
actor_effects and brick_draw_budget, all green on 76135c14.
Gate sent `53184367` back: its content check (`bri-client --check` over the
shared main checkout) installed the three into `content/addons` there, and
two tests counting 21 items then saw 22. Now `--check` changes nothing
unless `BRI_INSTALL_DEFAULT_ADD_ONS=1` opts in (the fresh-checkout test does,
on its temporary content, and also checks a plain `--check` writes nothing);
only running the game or `bri-server` installs. `app::tests::native_weapon_catalog_startup_and_headless_host`
and `content::tests::local_native_content_index_and_lazy_maps` now count
v20's 21 items plus whatever loaded Add-Ons add. The three folders already
in the main checkout's `content/addons` were left for Max.

## 2026-09-29 Total-conversion seams for Add-Ons (branch `claude/total-conversion-addons-jw91uo`)

Max wants Add-Ons able to turn the game into something else (Mario or Call
of Duty in Minecraft), with the seams ready before modders arrive. The
area-by-area audit is `docs/audits/total-conversion.md`: each seam is
present, partial or missing, and each gap is built here, planned next with
its reason, or marked not worth it.

Built:

- **Weapons, schema 3:** aim zoom (right-click aim, a hidden crosshair,
  forced first person), the Add-On's own sound files, `eye_rotation` for
  Add-On images.
- **Rules:**
  - hooks `on_spawn`, `on_leave`, `on_damage`, `on_entity_damage` and
    `on_entity_death`;
  - `heal`, `center_print`, `bottom_print`, `play_sound` and `sound_at`;
  - `fire`, which launches projectiles from rules and creatures. A package's
    own shot hurts any living player and credits nobody.
- **Entities:** creatures are hit by guns, hammers and blasts.
- **Bodies:** archetypes may have no body; package models draw scaled.
  The horse is detected by look.
- **Client code:** view and screen spaces, `view`, and each player's
  archetype, held image and crouch.

Correction (the coordinator's test: could a modder have built it from
generic pieces?): a first cut put magazines, reloads, an ammo counter and
view kick into the engine. They are gone, and by Max's call (2026-09-29:
keep what looks good, no chase) nothing replaces them: the Commando rifle
is a plain scoped rifle, and magazine seams are listed as future work.

Protocol 56 (54 and 55 are reserved for the gating batch; the Gate owns the
final number).

Evidence: the Commando sample, five Add-Ons in `packages/samples`, played
headless:

- `crates/sim/tests/commando.rs` (4 tests);
- `crates/client-sandbox/tests/commando.rs`, whose ignored test renders the
  sights offscreen, checked on Mesa's software Vulkan;
- `the_commando_sample_loads_as_one_game_mode`;
- unit tests in `bri-weapons`, `bri-client`, `bri-ui` and `bri-net`;
- `every_operation_needs_its_declared_capability` now covers `fire`.

Defaults picked:

- aim is the zoom key, plus the right mouse button when the image asks for
  it;
- a creature cannot be shot by its own driver;
- fire does not burn creatures;
- Commando falls hurt half (sample only).

Future work, by Max's call to keep what is good and not chase the rest
(listed in the audit): custom movement, side-on cameras, animated box
models, drawn custom blocks, and magazines, reloads and recoil for Add-On
guns. Smaller gaps, with reasons, are in the
audit's tables.
## 2026-09-29 Docs polish (branch `claude/project-thread-j2okix`)

Max asked for the README and docs to be tidied. Built on the cleanup pass's
docs commit (`163ead16`, cherry-picked so both merge cleanly), then:
README gains a "What's in it" summary (v20 fidelity, big builds, quality of
life, Add-Ons with the Stunt Plane and Duplicator shipped on, generic hooks
and the Commando total conversion, direct hosting) and points modders at
`audits/total-conversion.md`; FEATURES adds big builds, the shipped
Add-Ons, the total-conversion hooks and automatic `.bls` conversion;
STATUS drops the stale a17 build pointer and the feature-freeze note, adds
a release-state section and Max's standing decisions (default Add-Ons,
generic hooks, performance headline, event limits, budgets, deterministic
tests), and lists the bug-pattern, v20-behaviour and total-conversion
audits; the docs index links them too. Links to `audits/total-conversion.md`
and `audits/v20-behaviour.md` resolve once those lanes land. Docs only.

## 2026-09-29 First person in vehicle seats (branch `claude/vehicle-first-person`)

Max reported the first-person camera in the wrong place in the Stunt Plane
and the Tank's driver seat. Every rider saw from their seat plus a fixed 1.6
along the seat's up. Read from blocklandv20.exe: `Player::getCameraTransform`
(0x5ab7d0) at `pos` 0 gives a rider whose control object is a vehicle
(type 0x4000) and who is mounted on it the seat's mount node translation
(times the vehicle's scale) plus the rider's animated `eye` node translation
(times the rider's scale), both in the vehicle's frame, placed by the
vehicle's transform. Everyone else (passengers, the Tank gunner on
TankTurretPlayer, horse, rowboat and cannon riders) takes
`Player::getRenderEyeTransform` (0x5aafa0): their own transform, the mount
node's, times the posed `eye` node. So the eye follows the seat's
`mountThread`: `root` seats (Tank, Jeep back, Stunt Plane wings, horse,
cannon, skis) see 2.16 above the node and 0.14 ahead, `sit` seats 1.76
above and 0.17 back. The Tank's driver saw 0.56 too low, inside the hull; the
Stunt Plane's pilot 0.16 too low and 0.17 too far forward.

Code: `App::rider_eye` reads the local rider's posed `Eye` node each frame
after posing; `vehicle_camera::driver_eye` places a driver. Data-driven from
the seat's mount node, its pose and the rig; no vehicle is named. Defaults:
vehicle scale is taken as 1 (the client does not know it); the 1.6 stays
only as a stand-in before the body is first posed. Not changed: v20's view
also rolls and pitches with the seat (the rotation is the rider's transform
times the head); our first-person view has yaw and pitch only.

Evidence: `cargo test -p bri-client --lib vehicle_camera -- --include-ignored
--nocapture` prints every seat of the stock vehicles and the Stunt Plane
(35 seats) with its v20 eye; `cargo test -p bri-client --test
vehicle_first_person -- --ignored --nocapture` boards a Tank as a joined
guest (driver, passenger, gunner in turn) and as the host (driver) on a LAN
game and checks the rendered camera against the eye built from the pack and
rig, within 0.05. Needs Max's in-game check in the Tank and the Stunt Plane.
## 2026-09-29 Riding a horse player dropped every client (branch `claude/project-thread-ikoj1o`)

Max's v0.1.0-alpha report: getting on a player turned into a horse showed
"Invalid vehicle cue" and dropped the game. The host plays `player.mount` as
a `VehicleSound` cue with vehicle 0 when the mount is a player (a player has
no vehicle), but `Cue::validate`, which every client runs on each replicated
delta (`bri_net::replica`), required a vehicle id above 0, so the whole delta
was rejected on the host's own client and on joined clients alike. The
existing riding test only read the host's cues, so it never ran that check.
Fix: `VehicleSound` accepts vehicle 0 (clients place the sound by position
and never read the id); `VehicleEffect` still needs a real vehicle. No wire
change. `Cues::emit` now debug-asserts each cue passes the client check, so
any host path emitting a cue clients would reject fails its tests. The
riding test validates every cue; it failed with the old check and passes
now. Tests: `cargo test --no-fail-fast -p bri-sim -p bri-chaos -p bri-net`
all pass in a content-free cloud checkout except `bri-sim --test tools`,
which needs the generated weapons pack (unchanged by this work).

## 2026-09-29 Old saves converting to 0 bricks (branch `claude/bls-zero-bricks-r4nsup`)

Max: some of his old `.bls` saves list and load with 0 bricks; his guess
was custom (Add-On) bricks sinking the whole save. The reader already kept
an unknown brick aside (unresolved, saved back, placed on a server that has
it), so one custom brick could not empty a save. What could: the reader
held every line to v20's exact 12-field layout and 0/1 flags, and skipped a
line that differed. A save whose every line differs the same way (a version
that wrote fewer fields, an Add-On datablock that writes an empty
`isBasePlate`, `true` for a flag, a colour or effect a version did not
have) came out with 0 bricks. Other whole-save failures: any byte
0x80..0x9F (Windows-1252 quotes, dashes, `™` in a description or an
Add-On brick name) refused the file; a missing `Linecount` line or an
unreadable colorset line refused it; a brick with an event row index past
the native 1024-row limit failed the whole save in `events::bind`.

Fix, following v20's own `getWord` reading (and Brickadia's `bl_save`
reader, which reads the community's saves the same way):
- Brick lines need only a name, the delimiter and a readable position.
  Missing words take their defaults (effects 0, raycast, collision and
  rendering on); words after the twelfth are ignored; flags read as
  `dAtob`; an angle is taken modulo 4; a colour outside the colorset and
  unknown effects become the default. The source record says when an older
  layout was adapted.
- Text that is not UTF-8 is Windows-1252. `Linecount` may be anywhere or
  absent. Colorset lines read leniently; a 0..255 line is scaled.
- Skipped lines are counted by reason (`bls::Skipped`), logged per save.
- `events::bind` leaves rows past the per-brick limit out (counted) instead
  of failing the save. Too many extension lines stop being kept, not the
  save.
- `CONVERTER_VERSION` 3, so saves already converted with 0 bricks convert
  again on the next start.

Not changed: bricks off the stud/plate grid are still skipped when placed
(counted in "created / total"). Unverified against Max's own saves (his PC
was offline): the Gate should run the game once with his saves folder and
read the console's "skipped brick lines" and "Skipped old save" lines.

Evidence: `cargo test -p bri-bls -p bri-convert --lib` (new cases: custom
and stock bricks together, shorter older lines, no `Linecount`, odd values,
Windows-1252 text and a 0..255 colorset, skip reasons, event rows past the
limit); `cargo test -p bri-chaos --test bls_fuzz` (new, fixed seed: random
mixes of stock, custom and eight-bit-named bricks, shorter and longer lines,
extensions and broken lines keep every readable brick; garbage never
panics); `cargo test -p bri-client --lib saves`; `cargo test -p bri-chaos`.

## Tools and weapons against v20 (2026-09-29, branch claude/tools-v20-audit-jkij5k)

Max's report: holding fire with the spray can and scrolling colours keeps
spraying in v20; ours stopped. Cause: the trigger lived in the mounted image
and every mount started it released; the host also dropped queued trigger
edges on every equip. Fixed at the model level in `bri-weapons`: the trigger
is the actor's held button, copied to image slot 0 every tick (v20
`Player::updateMove`); a mount while the held image's state forbids changes
waits (Torque `nextImage`) instead of being refused; putting away is
immediate; mounting the held image again is a no-op. The client keeps its
fire-down flag through equip acknowledgements so the release still goes.
Full item-by-item audit and deferrals: `docs/audits/tools-v20-audit.md`.
No wire protocol change.

Evidence: `cargo test -p bri-weapons --test held_trigger` (8 content-free
cases); stock-pack `a_held_spray_can_keeps_spraying_through_scrolled_colours`
and the rewritten rocket re-equip case; host
`scrolling_the_spray_can_while_holding_fire_keeps_spraying`; client
`a_trigger_held_through_tool_and_colour_switches_is_still_released`. The
session test helper `swing` now releases (new trusted `Session::release_trigger`)
and waits for Ready, since a held hammer auto-repeats as in v20.

## 2026-09-29 Vehicle behaviour against v20 (branch `claude/vehicle-v20-audit-mg3f1h`)

Maxwell's v0.1.2 report: the mouse is inverted in vehicles; the Stunt
Plane's first-person view stays level while the plane loops; "many other
little things". He rates the Tank's driver seat good. The full checklist,
with a verdict for each item, is `docs/audits/vehicles-v20-checklist.md`.

- Evidence read this time: blocklandv20.exe from the reference install.
  - `Player::getCameraTransform` 0x5ab7d0, `getRenderEyeTransform` 0x5aafa0,
    `Vehicle::getCameraTransform` 0x56cc10.
  - The move split in `processTick` 0x5b2cad and the head update in
    `updateMove` 0x5ae972.
  - `isFirstPerson` 0x526d60, and the type masks registered at 0x59b65c:
    PlayerObjectType 0x4000, VehicleObjectType 0x10000.
  - Stock and reference client defaults.
- Inverted mouse. Stock v20 ships `VehicleMouseInvert = 1`. The designated
  reference install (`base/client/defaults.cs`) and stock v21 ship 0. We used
  the UI pack's stock 1, so moving the mouse up dipped a plane's nose.
  - `NATIVE_DEFAULTS` in `bri_ui::screens::options` now replaces pack
    defaults, starting with `VehicleMouseInvert = 0`. Options saves only
    changed values, so a player who never touched the box gets the new
    default.
  - Kept at stock 1: `UseStrafeSteering` and `UseAutoReturnSteering`. The
    reference install has 0, but Maxwell rated the strafe-steered Tank good
    and v21 keeps 1.
- First-person view. In v20 every rider of a vehicle sees through the seat:
  the seat's rotation times `rotZ(mHead.z)·rotX(mHead.x)`, so the view
  rolls and pitches with the vehicle.
  - `updateMove` halves `mHead` every 32 ms tick while a vehicle's rider is
    in first person and not free looking, and leaves it in third person.
  - `controls::Ride::Seat` carries the seat's rotation. A new head pitch
    springs back in `advance_head`. The renderer takes a roll
    (`rolled_view_basis`).
  - A seated rider sends the view's world yaw and pitch, so tools aim at the
    crosshair.
  - The Tank gunner's view rides the hull (`Ride::Hull`). Player-type mounts
    (horse, rowboat, cannon, turret) stay upright and unsprung.
  - The last camera fix (06ed4ce7b) had the type masks swapped. The
    mount-node eye is for riders controlling a player-type mount, not vehicle
    drivers. Positions were equal on every stock seat.
- Free look while mouse steering fed the steering. v20 gives the vehicle no
  yaw or pitch while free looking. Fixed.
- In third person a mounted player hands the camera to its mount
  (0x5ab80e). Passengers and the Tank gunner now see the vehicle's chase
  camera instead of an orbit round their seat. Accepted difference: each
  rider's own free look swings their view, where v20 used the newest rider's
  head for everyone.
- `Armor::doDismount`:
  - The first exit point is 2.2 up the rider's tilted transform.
  - A rider is never refused: with every point blocked they land at the
    last point tried with no push.
  - The velocity carried is the vehicle's, without its spin.

  We used world up, refused blocked dismounts and added the spin.
- Next and Previous Seat on foot, on a one-seat mount or with no free seat
  now do nothing silently, as `serverCmdNextSeat` does.
- Protocol unchanged: no new messages or fields, and no per-tick traffic.
- Evidence:
  - `cargo test -p bri-vehicles --test native`: 31 pass, including the new
    `a_blocked_dismount_takes_the_last_point_without_a_push`,
    `the_first_dismount_point_is_up_the_tilted_seat` and
    `dismounting_a_spinning_vehicle_hands_on_its_velocity_only`.
  - `cargo test -p bri-client --lib controls`: new tests for the rolled seat
    view, the head's spring, the tilted aim, free look while mouse steering,
    and the gunner's hull view.
  - `cargo test -p bri-ui --lib options`: the default is off.
  - `cargo test -p bri-client --test vehicle_first_person -- --ignored`:
    every Tank seat's eye and view rotation, and the passenger's
    third-person chase camera, for a LAN guest and the host.
- Feel checks for Maxwell are at the end of the checklist.
- 2026-09-29 First-person held images follow the arm's actions (branch
  `claude/fp-brick-animation-col45e`). Maxwell saw the grey brick in hand
  jolt in third person as he shifted, rotated and planted the ghost brick,
  but hold still in first person. Cause: an image with an `eyeOffset`
  (brickImage, hammer, wrench, sword, wands, spray cans, printer, skis) was
  placed at `eye * eyeOffset` in first person, so the thread-2/3 arm actions
  (`shiftAway`, `rotCW`, `plant`, `armattack`, ...) that move it in the hand
  never reached it. Images without an eye offset (guns, bow, spear, balls)
  already sat in the animated hand in first person. Now the avatar also
  samples each pose without its action layers while one plays
  (`AvatarMesh::mount_action`, the mount's motion in its own frame), and
  `ItemAssets::moved_mount_transform` gives the eye-offset image that same
  motion in its own frame. Client-side and cosmetic; no wire change. The
  exact closed-engine formula is inferred from what v20 shows; hammer and
  other melee first-person swings now also carry the arm's swing on top of
  their `detail9999` clip (feel check for Maxwell). Evidence: `cargo test -p
  bri-client --lib` (206 passed); content tests `cargo test -p bri-client
  --test v20_poses -- --ignored first_person_eye_offset` and `--lib
  mount_action -- --ignored` (both pass on Maxwell's content).
  Placement effects, same branch: Maxwell saw effects missing when placing
  the ghost. v20's click fires `brickImage`: its Fire state streams
  `brickTrailEmitter` for 0.1 s from the brick in hand and
  `brickDeployProjectile` bursts `brickDeployExplosion` (blue chunks and a
  white-to-black light) where the ghost lands; moves, rotations and plants
  play thread-3 arm gestures and the engine's BrickMove/Rotate/Change/Plant
  sounds. All were wired except the trail: brickWeapon.dts has no
  `muzzlePoint`, so its cue waited for a pose and was dropped. Torque's
  `getMuzzleTransform` uses the image's own transform then;
  `WorldItems::effect_pose` now does too, for every image without a muzzle.
  Evidence: `cargo test -p bri-client --release --test night_qa -- --ignored
  placing_the_ghost_shows_the_brick_trail_and_puff` (new; first and third
  person each accept the trail and the explosion, no missing poses; the host
  steps on the wall clock, so the test gives it real time),
  `--test world_items -- --ignored` (new
  `the_brick_trail_streams_from_the_held_brick_without_a_muzzle_point`), and
  `bri-sim --test session -- --ignored an_aimed_click` (new), all on
  Maxwell's content.
## 2026-09-29 Old saves failing with "Unresolved native print NOPRINT" (branch `claude/noprint-load-fix-yyt1yg`)
- Cause: since the 0-brick fix, many more old `.bls` brick lines load, and
  they carry print names the stock bundle cannot resolve (`NOPRINT`,
  `base/data/prints/Letters/A.png` paths, Add-On prints). One such brick made
  the client's chunk build fail, so the join ended in Connection Failed.
- Fix: `world_scene::print_material` draws any print the bundle cannot resolve
  with the blank print surface and logs each name once. Test:
  `unknown_prints_draw_blank_instead_of_failing_the_world` (fails on the old
  renderer, passes now).
- New headless probe `saves_host_probe <content> <saves-dir> <report.json>`
  hosts every save as the game does (background conversion, Load Bricks, a
  host session loading to the end, then the client's chunks and collision
  mirrors).
- Evidence on Maxwell's 699 saves: before 395 failed (357 NOPRINT, 34 other
  unknown prints, 4 empty saves); after 4 failed, all "Build contains no
  bricks". Violin loads 5,018 of 5,018 bricks.
- Follow-up (3bcf130 and after): `Bundle::resolve` maps older saves' stock
  print tokens (`base/data/prints/<class>/<name>.png`,
  `Add-Ons/Print_<class>_<pkg>/prints/<name>.png`, renamed classes
  `2x2`->`2x2f`, `2x1`->`1x2f`, `1x1r`->`2x2r`, each holding exactly its v20
  package's image names) to the stock print; counters read their digit
  from them. Tests: `older_saves_print_paths_and_renamed_classes_resolve_to_stock_prints`,
  `counters_read_their_digit_from_any_print_name`.
- Re-run on 699 saves: 4 failures (the empty saves). The 59 names the
  first probe flagged by class were checked one by one against the 77 stock
  prints in `docs/research/v20-inventory.json`: none is a v20 print (Add-On
  packs using stock class folders: `Floor_*`, `BAN_*`, `*lcase`, `FART_*`,
  money, extra symbols). Two are stock image names in a class v20 never had
  them in (`2x2f/computer1`, `1x2f/Square`); v20's printNameTable has no
  such entry either, so they stay blank. The probe now reports only those
  near misses instead of every name under a stock class.
- Next: 399 saves name prints the bundle does not resolve; many are stock
  prints stored as `base/data/prints/<aspect>/<name>.png` paths (Letters/*
  in 190 saves) or older aspects (`2x2/`, `2x1/`, `1x1r/`). They now draw
  blank where v20 shows the image; mapping those names needs v20's loader
  rule as evidence.
## 2026-09-29 Chaos tests that hung or changed run to run (branch `claude/event-fuzz-deterministic-8v4hln`)

The Gate saw `event_fuzz::the_same_programs_play_out_the_same_twice` run
past 600 s on an unrelated branch. The proptests drew a fresh random seed
every run, so a rare program decided how long the gate took. Searching 40
seeds found one (16778118630780010966) that took 29 s alone in a debug
build: relays feeding each other filled the event queue to its 131072-row
limit, and from then on every tick's 4000 retried relays were each turned
away only after `can_commit` walked every held row to count origins, about
1 s per tick. That is an engine cost, not a test artefact: a relay loop
that fills the queue made each host tick's work grow with the queue.

Fixes:
- Events: admission refuses from the counts first (at most the cancelled
  sources' delayed rows can make room), and held jobs keep a per-origin
  count, so a full queue turns a row away without walking the queue. The
  same seed now takes 5.6 s; ticks at the limit went from ~1 s to ~0.1 s
  in debug. Admission decides exactly as before.
- Every bri-chaos proptest now draws from a fixed seed
  (`bri_chaos::proptest_config`); `PROPTEST_RNG_SEED` still picks others
  for a soak. The storm seed is its own test,
  `relays_that_fill_the_queue_keep_the_host_stepping`, and the
  same-twice test's pinned case loops until rows wait on the budgets.
- `session_chaos` also differed run to run: build loads placed what fit
  the tick's wall time. The chaos runner now loads a fixed 512 bricks a
  tick (`LoadPace::Bricks`), so a seed plays out identically, also with
  every core busy.
- That exposed a real bug on seed 0x13c6ef372: a vehicle weapon with no
  sound or effect (allowed by the pack schema; the chaos vehicles have
  none) emitted empty sound and effect cues, which clients refuse by
  dropping the connection. Firing now skips them
  (`vehicle_weapon_cues` test).

Evidence: `cargo test -p bri-chaos -p bri-events` all pass (event_fuzz
10.5 s); session_chaos reports hash-identical across 5 runs, one under a
6-process CPU load; `cargo clippy -p bri-events -p bri-chaos -p bri-vehicles
--all-targets -D warnings` clean. bri-vehicles' content tests need the
generated vehicles pack, absent in the cloud checkout; the Gate runs them.
## 2026-09-29 General script API for Add-On guns (branch `claude/gun-script-api-2a4up5`)

Max's friend, a modder porting a gun pack, sent a spec of script calls
("the vars I needed for guns... doesn't have to be exact, just
equivalents"; later "ignore the gun-pack specific stuff, I just need the
new functions in general that any mod can use"). None of the spec's code
existed on main, on GitHub or on Max's PC (all worktrees and 291 refs
searched). Each request was judged as a general building block; the
verdicts are in `docs/modding/torque-equivalents.md` ("Design notes"),
beside a new TorqueScript equivalents table.

Built:
- `raycast(from, dir, range[, ignore])`, answered during the call through
  the weapons' own sweep (`script::World`, `session/script_world.rs`), at
  most 64 rays of 2000 units per call. `Runtime::call` now takes `&self`
  and enforces the operation budget in the progress callback, so the
  session is only read while a script runs.
- `can_damage(by, target)`: the minigame damage policy for players,
  vehicles and entities.
- `damage(target, amount[, by[, type]])`: any player, vehicle or entity,
  with an optional weapons-pack damage type (`Op::Damage` replaces
  `Op::DamagePlayer`). Unknown types are refused.
- Player facts `mounted`, `scale`, `cx`/`cy`/`cz`, `slot`, `image`,
  `image_state`.
- `mount_image` (keeps the tool slot; `()` restores the tool's image;
  own or dependency images only), `set_image_ammo`, `set_fov` (5 to 120,
  `Notice::Fov`; the client uses it in place of the normal FOV).
- `effects` capability (replaces `sound`): `play_sound`, `sound_at`,
  `beam` (`CueKind::Beam`, a coloured beam that thins and fades, started
  at the shooter's drawn muzzle, `crates/client/src/beams.rs`) and
  `play_thread` (threads 2 and 3, as `WeaponAnimation` cues). All share the
  64-a-second cue allowance.
- Image `commands.light`: the light key runs the held image's command.
- Converter: `-1` pool starts in DTS sequences read as unused
  (`convert/src/shape.rs`). Import Add-On's test shot clicks when the gun
  is ready instead of at 0.5 s (`addon-import/src/porting.rs`).

Protocol: `CueKind::Beam` and `Notice::Fov` are new wire variants; the
Gate numbers the protocol.

Later: converting an Add-On's own particles, explosions and AudioProfiles
on import (importer work), custom casing models.

Evidence: `cargo test -p bri-sim --test script_api` (new: rays, ignore,
map and misses, the 64-ray cap, damage rules and types, held-image facts,
ammo, scope swap, foreign images refused, light key, FOV notices, beam and
animation cues); `cargo test -p bri-package-runtime` (new
`script_calls_build_their_operations_and_world_questions_need_a_world`,
every new op in the capability and bounds tests); `cargo test -p
bri-chaos --test script_effects_fuzz` (new, fixed seed: anything the gate
accepts becomes a cue clients accept); client `beams` and
`host_fov_replaces_the_normal_fov_until_handed_back`; `cargo test -p
bri-convert --lib shape`; `cargo test -p bri-addon-import --lib slow`;
`cargo clippy --workspace --all-targets -- -D warnings` (only the
existing Linux-only `sampler.rs` unused import remains).

Follow-up on the same branch: three content tests the Gate saw fail once
in a batch and pass alone. `app_flow`'s rehost sat on "LOADING MAP" past
its 15 s deadline under load (the MessageBox over it is the intended
"load canceled" for the stale Load Bricks the test sends); each wait now
shares one hang-only deadline. `motion_probe` sampled for a fixed 20 s,
which a loaded PC could spend loading; it now samples until the server has
held the player still for 120 samples. `shadow_render` captured after
0.5-1 s of wall time; it now waits until the world mesh shows the latest
world, then runs a fixed 60 frames.

## 2026-09-29 Load Bricks: save pictures and the current map (branch `claude/saves-search-hso2qr`)

A tester saw only the map's picture in Load Bricks and had to pick the map
they were on. v20 (read on the PC from the decompile): `saveBricks` ends
with a HUD-less `screenShot("<name>.jpg")` beside `<name>.bls`, and
`LoadBricks_FileClick` shows that `.jpg`, else the default mission picture.
Max's v20 folders pair every `.bls` with a same-stem `.jpg` (stock ones
294x220; players' saves at window size). Now: picking a save asks the host
for `<stem>.jpg` beside the save (the original `.bls` for converted saves,
`<name>.jpg` beside `<name>.world.json` for native ones), read and scaled to
fit 588x440 on a worker, else the map's picture. Save Bricks takes the
picture on the next frame without the interface (scaled the same, JPEG); an
overwrite removes the old one first, as v20 did. Converted stock worlds have
no picture yet (their `.jpg` is not in the worlds pack). The save context
now names the map as the save list files it, the played map is always in
the map menu, and map names match in any case, so Load Bricks opens on it.
Tests: `bri-ui` saveload (picture per pick, opens on the played map),
`bri-client` `save_picture` and the saves overwrite test.
- 2026-09-29 Sharp Filter and help-page text (branch
  `claude/texture-filter-f1-text-n4c2bk`), from a tester's report.
  - Use Sharp Filter pixelated map textures up close. We applied it to every
    diffuse sampler (nearest magnification and minification). In
    `blocklandv20.exe` the pref (`gUseGLNearest` 0x8705e0, registered at
    0x58eb8b) is read only by the two brick draw paths (0x52ceb0, 0x4c7b40),
    and only for brick textures not flagged smooth: brickSIDE. Those always
    magnify nearest; sharp changes their minification from
    `GL_NEAREST_MIPMAP_LINEAR` to `GL_NEAREST` and lowers anisotropy. The
    texture manager (0x5098d0), which filters interiors, terrain, shapes and
    skies, never reads it. Now only brickSIDE follows the setting (its own
    sampler, base level only); everything else keeps smooth filtering.
  - F1 help pages showed one letter per line. Torque keeps `rmargin%:n` as
    the right edge's position (n% of the width from the left), while
    `rmargin:n` is n pixels from the right; we treated both as a distance
    from the right, so the pages' `<lmargin%:3><rmargin%:97>` left a 3% wide
    column. Fixed in the shared ML layout, so every ML control is covered.
  - Evidence: `cargo test -p bri-ui --lib ml::`, `cargo test -p bri-render
    --test texture_filtering --test shader_validation` (the scene shader now
    validates without a GPU).
## 2026-09-29 Stunt Plane controls and seat look (branch `claude/stunt-plane-controls`)

Maxwell's v0.1.2 test:
- The Stunt Plane felt as if "two systems are conflicting" when moving the
  mouse up and down.
- A Jeep passenger was frozen in place.
- The driver could not look up and down without Z.

Full walkthrough: `docs/audits/vehicles-torque-audit.md`. Per-item marks are
in `docs/audits/vehicles-v20-checklist.md`.

- **Root cause of the seat mistakes.** The first audit read two Player fields
  the wrong way round. +0x658 is `mMount.object` (written by
  `ShapeBase::mountObject` 0x5bf379). +0x864 is `Player::mControlObject`.
  - `Armor::onMount` gives passengers `setControlObject(%obj)`, which stores
    nothing (Torque player.cpp:1972).
  - So passengers have no control object. Their move is never split
    (0x5b2c81), their head pitch never springs back, and in third person
    they keep their own camera (0x5ab80e).
  - A strafe-steered driver's move is treated as free looking (0x5b2d15 to
    0x5b2d7a), so the mouse turns and pitches the head without Z.
  - Only a mouse driver's head springs back, and only in first person.
  - The gunner's third person is the turret's own camera.
  - `SeatLook` in `controls.rs` carries these rules. `driver_head_yaw` swings
    the chase camera.
- **The plane's "two systems".** Three things answered one mouse move:
  1. The head nudge moved the view at once, then sprang back.
  2. The plane itself answered a round trip later: the driven vehicle was
     drawn at its last pose, with no rotation extrapolation, and the warp
     held the old rotation.
  3. The host posed the pilot's arms from the steering accumulator, which
     flips every half turn.

  Fixes:
  - The nudge first came out, then went back in as v20 has it (0x5b2cd4,
    0x5aeae3) once the plane was predicted: it only fought the plane while
    the plane answered late.
  - The host keeps a mouse driver's body pitch level.
  - The client poses a seated body from the head.
  - The client predicts the vehicle it drives, as Torque does for a
    controlled object (vehicle.cpp:801, :1549/:1565):
    - `Predictor::drive` spawns the host's own `VehiclesWorld` vehicle in the
      collision mirror, with the rider made non-solid as the host's seated
      riders are.
    - It steps once per recorded input with `session::driver_controls`,
      shared with the host.
    - On each newer `VehiclePose` it restores the body, spin, steering
      accumulator and wheels (`VehiclesWorld::restore_motion`) and replays the
      moves after `driver_input`.
  - `Motion::driven_frame` interpolates the prediction and fades corrections.
    `ClientVehicles::set_predicted` draws it.
  - An unpredicted driven vehicle's rotation now also extrapolates by its
    spin.
- **Protocol changed.** The Gate numbers it.
  - `VehiclePose` gains `angular_velocity`, `mouse_steering` and
    `driver_input` (28 bytes per pose).
  - `VehicleInfo` gains `scale`.
- **Not run:** the suggested headless v20 measurement. The disputed paths
  need a connected client (control object, first person), which a dedicated
  server with bots does not exercise.
- **Evidence:**
  - `cargo test -p bri-sim --test vehicle_prediction`: the Flying Wheeled
    Jeep pitched by a scripted mouse under a 100 ms round trip, with a pose
    every third tick. The prediction stays within 0.00002 of the host and
    answers on the input's own tick.
  - `cargo test -p bri-client --lib controls`: the passenger, strafe-driver,
    mouse-driver, gunner and rolled-view tests.
  - `cargo test -p bri-client --test vehicle_first_person -- --ignored`:
    every Tank seat for a LAN guest and the host, the passenger's own
    third-person camera, and the predicted driver seat within 0.006.
- Feel checks for Maxwell are in the hand-off.
## 2026-09-29 Wrench dropdown search takes typing
Max, testing v0.1.2-alpha: the wrench's light, emitter, item and event
dropdowns showed a search caret but typing entered nothing (Load Bricks
search worked). The dropdown's type-to-filter lived in `View::char`, but the
platform only forwarded typed characters (and enabled the IME) while a
`GuiTextEditCtrl`/`GuiMLTextEditCtrl` had focus, so an open dropdown never
got them. `View::takes_text` (a focused text box or an open dropdown) is now
the one rule, read through `Ui::takes_text` by the platform, and the IME
window sits under the open dropdown (`View::text_node`). Test
`typing_reaches_an_open_wrench_dropdown_search` opens the wrench through
`Ui`, clicks the lights dropdown and types key/char/key-up like the
platform. Checks: `cargo test -p bri-ui`; `cargo clippy -p bri-ui
--all-targets` and `-p bri-client --lib --bins` with `-D warnings`.
## 2026-09-29 Non-rendering bricks shown as outlines (branch `claude/hidden-brick-look-22pf5w`)

Max (v0.1.2 playtest): with a brick's rendering off, taking out the hammer
showed it as the blinking ghost brick; he remembered v20 drawing something
else. Read-only disassembly of the reference `blocklandv20.exe` confirmed it:
v20 outlines each hidden brick's world box with one-pixel lines in its paint
colour, unlit and steady (details in `docs/audits/bricks.md` finding 5b). The
tools that reveal them were already right (hammer, wrench, wands, printer,
held bricks; not the spray can).

Change: new `bri_render::lines` (line-list pipeline, one vertex buffer
rebuilt only when the hidden set, the tool or a fading brick's outline state
changes, depth tested, no depth write, drawn in the pass after the world).
The client builds 12 edges per hidden brick from its footprint and height
instead of uploading a ghost mesh. v20's fade is ported too: turning
rendering off eases the brick's alpha to 0 on the existing repaint curve
(`brick_fade::shown_color`), outlined from alpha 0.1, mesh dropped below
0.03; turning it on fades it back in. Client-only; nothing new on the wire.

Evidence: `cargo test -p bri-render --lib lines`, `cargo test -p bri-client
--lib` (new `rendering_off_fades_out_and_back_in`,
`a_faded_out_brick_draws_no_mesh`), `cargo clippy -p bri-render -p
bri-client --lib --bins -- -D warnings` (clean; `--all-targets` only trips
the existing Linux-only `sampler.rs` unused import).
## 2026-09-29 Autosave removed (branch `claude/remove-autosave-3xvm8v`)
Max, testing v0.1.2: autosave kept making new saves and wasting space; v20
never auto-saved (it was an Add-On), so remove it. Removed: the host's
`ServerOptions::autosave` timer, the save before an admin map change and the
save when the host loop errors (`crates/net/src/server.rs`); the client's
autosaver and the "keep the final world" save when a hosted game ends
(`saves.rs`, `app.rs`, `network.rs`); `bri-server`'s 60 s autosave;
`persistence::autosave*`; the Load list's "Autosave" label; and their tests.
Kept: manual Save Bricks, the unsaved-changes prompt (its text no longer
promises an autosave), and `bri-server`'s save on shutdown with `resume`.
Old `autosave-*.world.json` files are not deleted; they list under their
file name in Load Bricks and can be deleted from the map's save folder.
Checks: `cargo clippy --workspace --all-targets -- -D warnings` (only the
existing Linux-only `sampler.rs` unused import); `cargo test -p bri-world
-p bri-ui --lib`, `-p bri-ui --test runtime_input`, `-p bri-client --lib
saves`, `-p bri-client --test transport`, `-p bri-net --lib`, `-p bri-net
--test loopback admin_change_map`, `-p bri-net --test state_limits heavy`.

## 2026-09-29 Vehicle controls, third pass (branch `claude/vehicle-controls-r3`)

Maxwell's v0.1.3 test:
- The plane's mouse up and down felt inverted.
- The first-person camera shook while steering the plane with the mouse.
- A Jeep driver's mouse should steer like A/D.
- Passengers could not look left and right.

Maxwell decided against measuring the real v20 client, so this pass works
from the exe and the Torque source. Nothing was launched.

- **Plane pitch.** Measured through the app (host a LAN game, take off in a
  Flying Wheeled Jeep, mouse up), the default then raised the nose. Maxwell's
  saved settings (`%LOCALAPPDATA%\BlocklandReImagined\settings.json`, read
  only) carry no invert or steering keys. The code was right for the default
  it had; the default was the difference:
  - v0.1.1 and earlier: `VehicleMouseInvert` on (stock v20).
  - v0.1.2 and v0.1.3: off (the reference install and v21).
  - Now on again, stock v20: mouse up dips the nose.

  App test `invert_mouse_in_vehicles_turns_the_nose_both_ways_through_the_app`:
  with the default and with the box ticked, mouse up dips the nose in the
  host's pose and in the client's predicted view; unticked, it raises it.
- **First-person shake.** The host drained a seated player's whole input
  queue every tick. Walkers consume one per tick. Moves arrive two to a
  datagram, so the host's vehicle ran two moves' steering in one step and
  none in the next. The client predicts one step per move, so every pose
  corrected the view.
  - Session test `a_predicted_driver_needs_no_corrections_when_moves_arrive_in_pairs`
    measured up to 0.33 units and 0.009 rad per pose before, and none after.
  - Seated players now consume one move per tick (three with a backlog).
- **Jeep driver.** `steeringUseStrafeSteering` defaults on in the engine
  (0x5716ec), so the Jeep and the Tank steer by the strafe keys while the
  client's `$pref::Input::UseStrafeSteering` is on. With it off, the mouse
  steers (0x5b2d15 to 0x5b2e97).
  - Stock v20 and v21 default it on. The reference install and Maxwell's
    own v20 `config/client/prefs.cs` have both steering prefs off.
  - `NATIVE_DEFAULTS` now sets `UseStrafeSteering` and
    `UseAutoReturnSteering` to 0. The Tank steers by the mouse too.
  - Session test `the_jeep_steers_by_the_mouse_without_strafe_steering_and_by_the_keys_with_it`.
- **Passengers.** A passenger has no control object, so its turn reaches
  `mRot.z` (0x5aeacd). Blockland's `Player::setPosition` (0x5a6bc0) draws a
  mounted player at the mount transform times `rotZ(mRot.z)`, as Torque's
  does. So the mouse turns a passenger's whole body on the seat, and Free
  Look turns only the head.
  - No per-seat rule was found (sitting against bumper seats); that absence
    is inferred.
  - The client sends the turn relative to the seat.
  - The host's `follow_seats` adds it to the seat's heading. A mount resets
    it, and the world yaw from before the client knew it was seated is
    ignored.
  - Every client draws passengers turned by their replicated yaw.
  - Tests: controls `a_passenger_turns_on_the_seat...`; session
    `a_passenger_turns_on_the_seat_and_the_driver_does_not`.
- **Protocol** unchanged from `claude/stunt-plane-controls`: no new fields.
  A passenger's move yaw now means the turn relative to the seat.
- **Not done:** rowboat passengers (seated on a player-type mount) still
  face the seat. v20 turns them too.
- **Crash fix: "Predicted vehicle is gone".** Maxwell's v0.1.3 crashed
  when his skis wrecked.
  - The client's prediction copy ran the host's own wreck: `wreck_skis`
    removed the skis in the copy. The next step then failed, and
    `Predictor::record` passed the error up to the game.
  - `VehiclesWorld::set_prediction` now keeps a prediction copy from ever
    removing, wrecking or respawning a vehicle. The host's poses and
    listings decide those.
  - Any prediction failure (a vehicle gone, a bad pose, a refused mount) now
    stops prediction, logs it once, and shows the host's poses. It is never
    fatal.
  - Stopping never fails.
  - The client restarts prediction on any change of vehicle id, definition
    or scale, and stops it when the player leaves the driver's seat for any
    reason: dismount, death, disconnect or a seat switch.
  - Tests:
    - `crashing_predicted_skis_never_fails`
    - `a_bad_pose_stops_prediction_without_failing`
    - `player_type_mounts_are_refused_cleanly` (horse, cannon, turret)
    - `a_prediction_copy_leaves_wrecking_to_the_host`
    - `the_client_predicts_only_live_rigid_vehicles_it_steers` (every stock
      vehicle, destroyed, respawned, rescaled, redefined)
- **Closing the known items before v0.1.4** (Maxwell: finish what is known
  now).
  - **Every passenger turns on the seat**, including rowboat passengers and
    riders of a Horse Ray player they do not steer (`riding.rs`: the turn
    is stored, added to the mount's heading, and reset on mounting).
    Switching between passenger seats resets the turn on the client too,
    as `Armor::onMount` does.
  - **Player-type mounts are predicted** (horse, rowboat, cannon, standalone
    turret), from the motor's full state. `VehiclePose` gains `actor`, sent
    for those mounts only, so the protocol changes again; the Gate numbers
    it. The rider's move maps through `session::actor_controls`, shared
    with the host. Test `a_ridden_horse_is_predicted_and_agrees_with_the_host`:
    exact under a 100 ms round trip.
  - **`doSimpleDismount`** is read from any datablock.
  - **Getting off a player mount** with every exit blocked still gets out
    at the last point tried, as `Armor::doDismount` does.
  - **Free look on foot**, as v20: only in third person (0x5aea5f); the
    head's turn eases back instead of snapping.
  - **The one difference left:** the chase camera swings by the driver's
    own head. v20 uses the newest rider's head, which would let a
    passenger's Free Look move the driver's camera. The accepted defaults
    stay as agreed: the 1 s respawn floor, the cap of 5 map vehicle spawns
    on internet hosts, and the tire model.
## 2026-09-29 Fixed save corpus replaces the random pre-release sample (branch `claude/fixed-save-corpus-swgow1`)
Releases used to load a random sample of Maxwell's saves by hand. Now a
fixed corpus of 23 known-tricky `.bls` saves is hosted headlessly, as the
game hosts a dropped save, and the gate runs it whenever a change touches
saving, loading, the `.bls` reader/converter or brick and print data.
- `bri_client::save_host::SaveHost`: the hosting path `saves_host_probe`
  had (Load Bricks read, a host session loading to the end, the joined
  client's chunks, query mirror and prediction mirror), moved into the
  library so the probe and the test share it.
- `crates/client/tests/save_corpus.rs` +
  `crates/client/tests/save-corpus.json` (relative path, reason, expected
  bricks placed or expected refusal; no save content in the repository).
  Copies the listed saves to a temporary saves folder, converts them with
  the game's own background converter, hosts them four at a time (largest
  first, each worker with its own content) and checks every result. Skips
  with a "skipped:" line without `BRI_SAVES` (default
  `%LOCALAPPDATA%\BlocklandReImagined\saves`) or content. `#[ignore]`, and a
  gate `[[skip]]`, so the ordinary test pass never runs it.
- `tools/gate.py`: `SAVE_CORPUS_PATHS` next to the other rules; when the
  diff against origin/main touches one, the gate runs the corpus as its own
  step after the tests. `docs/release-builds.md` describes it and retires
  the random sample.
- Corpus picked from a full `saves_host_probe` run on all 700 saves (699
  listed, 695 hosted, the 4 empty saves refused as before): Violin; the
  only save with Windows-1252-only bytes (Awesome building Badspot); Latin-1
  brick names (A.T.C. Fort); the largest save (2023 XMas, 309,781 lines,
  172,982 placed); the most events (Sumz City) and most event rows on one
  brick (Icy Events); the 4 empty saves; a Duplicator selection placing 0;
  the most NOPRINT bricks (Apartment2); older stock print paths, FART_/BAN_
  Add-On print packs, lowercase-letter packs and the three stock-name,
  other-class near misses; an all-short-lines older layout; a name ending
  in a space; the renamed Slopes folder; Kitchen's frame-rate save.
- Found, not fixed: `Slate/Afghanistan DM .bls` and `Slate/afghanistan DM.bls`
  both list as "Afghanistan DM" on Slate (trailing space trimmed, names
  compared case-insensitively), so Load Bricks shows only one of them.
- Evidence: `cargo test -p bri-client --test save_corpus -- --ignored
  --nocapture` passes, 23 of 23, 97 s (conversion 18 s); skips cleanly with
  `BRI_SAVES` or `BRI_CONTENT` pointing nowhere; `cargo clippy -p
  bri-client --lib --bin saves_host_probe --test save_corpus -- -D warnings`
  clean.
## 2026-09-29 Brick shadows stay solid past map geometry (branch `claude/project-thread-ed9bi6`)
Max, v0.1.3 on Ultra (Best shadows, Brick Shadows on), Bedroom with
"Chonesis Paradise": the build's shadow on the east wall had straight-edged
lit wedges and a thin lit streak. Offscreen probe on his PC (normal / empty
occluders / map not in occluders): the wedge and streak were exactly the
Bedroom's wooden beam, which lies between the build and the wall along the
sun. Map interiors and terrain were in the occluder layer, so the beam
erased the build's shadow while casting none itself. Leaving the map out
made the shadow solid and matched an empty occluder layer.

Decision: the map (interiors, terrain) neither casts nor stops live
shadows; its shadows are baked, as engines with baked static lighting treat
static geometry for movable casters. Bricks that do not cast (Brick
Shadows off) still stop shadows, so the 2026-09-27 fix (a player's shadow
on a brick roof does not also land on the floor below) stays. A player on
a map shelf now also shades the floor beneath it, which is where the shelf
itself blocks the sun. Also: each cascade fades into the next over the
last 20% of its range (only receivers in that band sample twice), so the
step to coarser texels no longer shows as a straight seam; shadows in the
band are slightly softer than the near cascade.

Evidence: `cargo test -p bri-render --test shadow_occluders` (new
`cascade_splits_do_not_cut_a_shadow`, which fails with the blend band
disabled), `cargo clippy -p bri-render --tests` and `-p bri-client --lib
--bins -- -D warnings`; PC offscreen probe frames before/after and a
cascade seam before/after (not committed; personal save).

## 2026-09-30 Pong paddles stuck white after Load Bricks (branch `claude/project-thread-eilckt`)
Max: "pong events are broken again" (v0.1.4). All six Pong tests passed on
the gate and on his PC, also against the game's own conversion of his save.
A headless replay of the real Load Bricks path on his PC found the cause:
loading renumbers the save's colours onto the map's colorset (Bedroom 36 ->
70, black 16 -> 49), and `Session` built the event bindings' `palette_len`
once, at `set_event_catalog`. Every paddle relay's `setColor` row was
refused and disabled, so cells a paddle left stayed white. Wrench rows
using a loaded save's colours were refused too. Now the engine's colorset
size follows the world (`follow_palette` before any program is installed,
which rechecks every brick's rows; `validate_event_rows` uses the live
palette). Why it came back: the Sep 28 fixes were real, but every Pong test
loads the save as the whole world, where no renumbering happens (pattern 6
in `docs/audits/bug-patterns.md`). Evidence:
`events_in_a_loaded_save_paint_with_the_colours_it_brought` (content-free)
and `paddles_repaint_after_load_bricks_onto_a_map` (content), both failing
on 96fa4f9c5; `cargo test -p bri-events -p bri-sim`, clippy.
## 2026-09-30 Mirror bricks for Add-Ons (branch `claude/project-thread-vvxj2v`)
Max asked whether an Add-On could turn the window brick into a real mirror
(see yourself, see round corners). Built as a generic engine capability:
any brick datablock may name mirrored sides (`reflectionFaces`,
`reflectionDepth`, `reflectionInset`, `reflectionTint`,
`reflectionStrength`; docs/modding/README.md section 7). The catalog keeps
it as a typed, validated `reflection` field; no genre or mod code.

Rendering: planar reflections, as engines draw flat mirrors. Coplanar
mirrors share one reflected pass; the pass uses the mirrored camera with an
oblique near plane at the mirror (nothing behind it shows) and a
projection cropped to the mirror's screen rectangle, at a fraction of the
screen size. The biggest planes on screen go live up to the Mirrors setting
(Off 0, Low 1 at half size, Medium 2 and High 3 at full size; distance 48/64/96 units); the rest are silver. Reflection views
draw other mirrors silver (no recursion). Shadows are shared with the main
view. `SceneRenderer` now holds several camera views; shadow cascades pick
by the shadow origin rather than the camera, so reflected views sample the
main view's cascades correctly. Client-only: no protocol change. In first
person the local player's body and a third-person copy of the held item
appear only in mirrors. New pref `$pref::Video::Reflections` (default 2),
tied to the quality presets (Low Off ... Ultra High).

Not reflected at first: particles, foliage, weather and client-code
layers (added the same day, below); hidden-brick outlines and name tags
stay out by design; a package-model body of the local player in first
person is still missing. Feel check is Max's (stand in front of a mirror brick; angle one
round a corner).

Evidence: `cargo test -p bri-render --test mirrors` on lavapipe (1x and 4x
MSAA: a card facing the mirror appears on its own side, a card behind the
mirror is hidden; Off shows plain silver), `-p bri-render --lib
reflection` (4), `-p bri-content`, `-p bri-convert --lib catalog`,
`-p bri-client --lib` (225 passed), `-p bri-ui --lib options`; `cargo
clippy --workspace --all-targets -- -D warnings` clean except the known
Linux-only `sampler.rs` unused import.

Follow-up (Max: "build the addon"): the **Mirror** default Add-On
(`packages/brick_mirror`), a "1x4x5 Mirror" in Special > Mirrors whose two
broad faces reflect. It ships no v20 geometry: a package catalog binding
without `native_mesh` now reuses the shape (mesh, and collision unless the
package bakes its own) of an already loaded brick with the same `mesh_id`
(`Definitions::load_with`), and its menu icon falls back to that base
brick's. Import Add-On now keeps a brick that inherits a base brick's
`brickFile` this way instead of dropping it, so the v20-style example in
docs/modding/README.md works through Import too. Defaults picked: mirror
set halfway through the brick (`depth` 0.5) with a 0.1-unit frame,
orientation fix 0 (the content test below fails if the window's differs).
Evidence: `cargo test -p bri-sim --lib definitions` (new
`an_add_on_brick_reuses_a_base_bricks_shape_without_copying_it`),
`-p bri-package` (default list), `-p bri-addon-import`, `-p bri-client
--lib`; `python tools/default_addons.py check`. Needs the PC's content:
`cargo test -p bri-client --test default_add_ons -- --ignored` (new
`the_mirror_is_the_base_games_window_with_mirror_faces`: same shape and icon
as the window, two broad mirror faces).

Follow-up (Max: "make sure the mirror actually works on the entire
environment characters vehicles particles bedroom interior sky"): the map
(interiors, terrain, sky, water), bricks, players, vehicles and items were
already reflected by the scene renderer's per-view pass. Particles, foliage,
weather and Add-On code's world-space layers now draw in every live mirror
too: `EffectsRenderer`, `WeatherRenderer`, `FoliageRenderer` and the sandbox
`LayerRenderer` hold per-view state (camera uniform, instances, runs), and
each mirror snapshots them from its reflected eye, with billboards turned
by the plane (`PlannedPlane::reflect_direction`) so they face it and sort
far to near for it (`WeatherWorld::snapshot_from` for rain and snow).
`WorldPass::after_all` records them last in each mirror's pass. Extra views'
instance buffers grow to what they need, not the player's full budget.
Evidence: `cargo test -p bri-fx-runtime --test mirror_sprites` (lavapipe:
a sprite behind the viewer appears in the mirror on its own side; nothing
without a live mirror), `-p bri-render --test mirrors`, `-p bri-fx-runtime
--test gpu_contract -- --ignored`, `-p bri-weather`, `-p bri-foliage`,
`-p bri-client-sandbox`, `-p bri-client --lib`; clippy clean except the
known Linux sampler.rs import. Real-content check: `cargo test -p
bri-client --test mirror_render --release -- --ignored --nocapture`
(Bedroom, a wall of five Mirrors, a red pillar, a burning brick and a horse
behind the camera; writes artifacts/mirror-render/mirrors-high.png and
mirrors-off.png).

Follow-up (PC GPU round 2): the Bedroom pictures were right in content (the
room, the player, the horse, the pillar on its own side, flame particles,
seamless across five mirrors, silver when off) but the reflection looked
hazy and soft. Cause: drawn at half (Medium) or three-quarter (High) size,
the reflection was upscaled and its textures read a coarser mip, so plaster
and carpet averaged toward grey. Medium and High now draw full size (the
pass is still cropped to the mirror, so its cost follows the mirror's share
of the screen); only Low stays half size. New `a_live_mirror_is_as_sharp_and_true_as_the_room`
(`-p bri-render --test mirrors`, lavapipe) requires every reflected pixel to
be the source colour exactly; it fails at half size. Also: a receiver
outside the shadow cascade its depth picks (behind the camera, which only
a mirror shows) now reads the finest wider cascade that holds it instead of
sampling off its map. The render probe now raises the Slopes scene on a
baseplate clear of the hillside (round 2 timed out there), waits on the
brick count rather than the horse, paints the mirror frames white so red
counts only the pillar, and reports render stats with Mirrors on.

PC round 3 (6ecb233): Bedroom reflections now sharp (pillar bricks, the
player's face, carpet texture readable; seams, sides and shadows right) but
still under a grey-green film: the borrowed window shape's translucent
glass drew over the mirror. `Reflection::replaces` now drops a full
mirror's own translucent surfaces lying across a mirrored side, so the
Mirror draws the window's frame without its glass (round 4 showed a first
version, limited to a slab round the mirror, missed the real glass) (content test; the PC
Add-On test asserts the mirror has fewer quads than the window). Slopes
built none of its bricks in rounds 2 and 3; Load Bricks finds a save by the
map's name as the save list shows it, so the probe now asks the save store
for that name, answers a colour check, and on failure prints the game's
chat, screens and pending requests. Round 4 then loaded Slopes ("The
Slopes"): sky, snowflakes, horse, pillar and flame reflect with no seams;
the raised baseplate hid the player, so the scene now stands on the ground.
PC round 5 (1a23b06): film gone, reflections as rich as the room in the
Bedroom and on Slopes (sky, snowy slope, snowflakes, player, horse,
pillar, flames); both PC tests pass (mirror 23 quads, no glass). One rim:
the window's opening (glass at x ±0.96, y -1.3 to 1.48, toward one side)
is larger than the 0.1-inset mirror, so a 1-4 px gap showed round each
pane. The Mirror now uses inset 0: the mirror spans the side and the frame
in front hides its edges.
PC round 6 (06b682e): both PC tests pass; Bedroom and Slopes pictures
right (rich colour; sky, snowy slope, snowflakes, player, horse, pillar,
flames; no seams, wrong side, black areas, bad shadows, or mirror past the
frame). On Slopes' dark frames a 1-2 px lighter edge remains inside each
pane in the live shots only. The surfaces and pipeline match the silver
shots, and the live pass's crop covers the whole (now full-side) mirror, so
no clear colour is sampled there: it is the frame's lit inner reveal, which
stands in front of the mid-brick mirror, seen in the reflection, as a real
recessed mirror shows it (inferred from the geometry, not measured apart).

## 2026-09-30 One lighting model for maps and bricks (branch `claude/realtime-map-lighting`)

Tester idea Max asked for: bricks and maps should share one lighting model,
live brick shadows should stop darkening the maps' baked shade a second
time, and v21-style specular should be added. Client-only: no protocol,
save or content-bundle change; derived data is computed from the player's
own map bundle at load and cached under `<state>/light-volumes`.

Findings:
- The .dif files keep no static lights. The map compiler baked them into
  each interior's own lightmaps and dropped the entities; the "animated
  lights" sections are empty on every stock map interior (two unrelated
  interiors have any). Only bedroom.dif, kitchen.dif and tutorial.dif
  carry authored light; the outdoor maps' interiors have none.
- The mission lightmap is exactly base + sun ambient + sun x N.L x baked
  visibility (saturating), on outside-visible surfaces (checked on Kitchen:
  no texel below base + ambient, and a per-texel replay reproduces it).
- Lights were recovered by inverse rendering (greedy candidate search with
  ray-cast visibility, pattern-search refinement, joint non-negative colour
  refit). The compiler's model has no cosine: fitted without it, lit texels
  are 15 levels off on Bedroom (25 with N.L), Kitchen 25 (29), Tutorial 12
  (29); squared and smoothstep falloffs fit no better than linear.

Fit error per map (levels 0-255, every covered lightmap texel; fitted vs
no lights): Bedroom/BedroomDark 2.0 vs 6.2 mean (rms 9.4 vs 25.0; lit texels
12.2), 24 lights in 6 s; Kitchen/KitchenDark 11.2 vs 27.0 (rms 23.2 vs
57.8; lit 24.7), 10 lights; Tutorial 4.1 vs 11.5 (rms 9.2 vs 33.7; lit
12.0), 23 lights. Kitchen misses its orange stove light (the fit stops
there); its light stays in the residual below.

Design (the stationary-light model engines with baked lighting use):
- `bri_render::map_lighting::decompose_sheet`: each mission lightmap gets a
  companion texture (material slot 9): static light and the sun share the
  bake let through. The mission lightmap still draws as is; Unified modes
  subtract only the sun a live shadow removes, so baked shade is never
  darkened twice. (A first version rebuilt the lightmap from the two parts;
  bilinear filtering of the saturating sum brightened texels next to
  saturated ones, up to 64 levels on Kitchen, so it was dropped.) Terrain
  recovers its baked sun share from its own lightmap.
- `map_lighting::Bake` fits the lights, gives the strongest visibility
  channels (7; overlapping ranges never share one), bakes a visibility
  volume (sun plus the channels, from the map's geometry; 2-unit cells,
  at most 2M) and a residual volume (the light the channel lights do not
  explain, gathered like the classic light volume). Bricks, players, items
  and vehicles then take ambient + sun x min(volume, live shadow) + the
  map's lights x their visibility (with N.L, which gives bricks their form)
  + the residual. Indoors the sun now reaches bricks only where it reaches
  the walls.
- Specular: no v21 install exists under E:\Downloads\B4v21Launcher\versions
  (only v20), so it is Blinn-Phong from the same lights, falloff and
  visibility, power 40, strength 0.3, on bricks, players, items and
  vehicles. Map surfaces and terrain get none: Max chose "faithful, with
  some tolerance" (2026-09-30), and a highlight on plaster walls read as a
  new look, so maps keep their baked appearance in every mode.
- Options > Graphics "Lighting:" (native `$pref::Video::Lighting`): Classic
  (the v20 look, pixel-identical to main), Unified, Unified+Shine (default).
  Not part of the Quality presets. Until a map's bake arrives, Unified
  draws as Classic.

Evidence:
- Classic is unchanged: `scene_snapshot` from the pre-branch release build
  and from this branch at the same views differ in 0 pixels on Bedroom,
  Kitchen, Tutorial and Slate.
- `cargo test -p bri-render --release` (new `unified_lighting`: a live
  shadow over baked shade leaves it within 2 levels, over baked sun takes
  it down to the static light, and Classic still darkens both;
  `map_lighting`: a point light baked into a room's lightmaps is recovered
  within a unit, its colour within 0.08 and its exact falloff, with the sun
  kept out of the closed room); `--ignored` stock fits and light volumes;
  `cargo test -p bri-ui --lib` (Lighting row, default, save, fits its
  section); `cargo test -p bri-client --lib --release`; clippy `-D
  warnings` on bri-render (tests), bri-client (lib, bins), bri-ui.
- Frame cost, `lighting_probe` (new bin; RTX 4070 SUPER, 1080p, Best
  shadows, GPU timestamps, median of 70 frames; the machine was busy):
  a synthetic 1,000,000-brick build on Bedroom, Brick Shadows off: inside
  the build (full-screen overdraw) Classic 26.2 ms, Unified 26.0,
  Unified+Shine 26.5; overview Classic 74.7, Unified 75.9, Unified+Shine
  77.4 (+0-1.6% and +1-3.6%). The first mode
  measured always reads low (the GPU settling after upload), so the probe
  measures Classic again last. Stock saves (Cottage, Town, Golden Gate)
  render under 1 ms GPU in every mode. So the default is Unified+Shine.
  (Wall-clock frames at 1M bricks are 70-300 ms in every mode; that is
  draw encoding, not lighting.)

Finished on the PC by the gate lane (2026-09-30; the lighting lane lost
PC access):
- Lamp shadows. In the Unified modes the nearest, strongest recovered map
  lights cast live shadows from bricks, players, vehicles and items: six
  perspective faces per lamp (a little wider than 90 degrees so filter taps
  at an edge stay in the face), drawn as tiles into extra layers of the sun
  shadow array (a fragment stage may bind only 16 textures; a separate array
  made 17). Budget by Shadow Quality: Best 4 lamps, High 2, Medium 1 (512
  per face), Low none; Classic never draws them. Lamps are picked each frame
  by brightness and reach around the eye, in view, with a 1.5x lead for
  lamps already casting so the choice does not flicker. On a lightmapped map
  surface a lamp shadow removes that lamp's share of the texel's static light
  (the compiler's no-cosine light), never more than the texel holds. Lights
  reaching past 200 units are the fit's broad fill, not lamps, and do not
  cast: their shadows were long grazing smears across Kitchen's cabinets.
- Objects keep half of a map light on every face turned to it, the rest
  following N.L (was full N.L): the walls were lit without a cosine, so
  bricks beside them read too dark. Kitchen's Town tower side went 156 to
  164 (Classic 204): Unified bricks there stay darker than Classic because
  Classic takes its baked light volume, Unified the fitted lights plus the
  residual (ignoring the visibility volume changed nothing, so that is not
  the cause). Bedroom builds under the lamp come out brighter than Classic.
  Left as the deviation Max allowed ("faithful with some tolerance").
- Map surfaces get no highlights in any mode (the lane's last change: a
  highlight on plaster walls read as a new look).
- Kitchen's stove. The fit ended at 10 lights: it seeds candidates on the
  brightest leftover texels, and the stove's orange (255,128,0 at most,
  inside the oven around x -455..-485, z 55..155) never outranked the white
  leftovers. When the brightest seeds find no light worth keeping, it now
  seeds on the most strongly coloured leftovers: 7 orange lights are
  recovered along the stove. Mean error per map (levels, every covered
  texel): Kitchen 11.17 to 8.65 (rms 23.2 to 18.2, lit 24.7 to 17.9),
  Tutorial 4.1 to 3.62, Bedroom 2.0 to 2.04 (unchanged: it fills its 24
  lights from the bright seeds). An always-on colour seed made Bedroom worse
  (2.24), so it is a fallback only. Bake format 2, so stored fits rebuild.
- Evidence: `cargo test -p bri-render --release` (new
  `unified_lighting::map_lamps_cast_live_shadows_in_unified_modes_by_shadow_quality`:
  a slab under a map light shades the floor to 25 against 146 open at Best
  and Medium, none at Low, none in Classic; `shadow::tests` lamp picking,
  faces and tiles); `--ignored` stock fits (Kitchen must find an orange
  light and stay under 9 levels). Offscreen renders of Cottage (Bedroom),
  Town, Pirate World and Haunted House (Kitchen) in every mode, close-ups
  included, in `BlocklandReImagined-worktrees/lighting-shots-v2` (not
  committed): lamp shadows land where the lamp throws them (Cottage on the
  wall behind it, Pirate World's hut on the wall), no acne or banding at
  brick scale, Shine adds only small highlights on bricks. Default stays
  Unified+Shine.
- Frame cost. Drawing every lamp face each frame cost 30% inside a
  1,000,000-brick build at the default settings (Best shadows,
  Unified+Shine: 35.9 ms GPU against Classic 27.2). Bricks now keep their
  lamp faces: a static chunk's face is drawn again only when its lamp or
  face changes, plus one face a frame in turn (a changed build reaches its
  lamp shadows within 24 frames at Best); players, vehicles and items draw
  every frame into their own half-resolution faces (256), and a receiver is
  lit where neither shades it. Kept tiles clear by a depth-1 triangle over
  their viewport, never their layer. `lighting_probe` now updates the camera
  every frame as the client does. 1,000,000-brick synthetic build on
  Bedroom, Best, GPU p50: inside the build Classic 27.4 ms, Unified 29.7,
  Unified+Shine (default) 28.4 (+3.8%); overview 77.4 / 74.5 / 74.2 (no
  cost). Stock saves stay under 1 ms in every mode.

Known gaps / next:
- Breaking the Bedroom bulb could now switch its light off (its fitted
  lights and their lightmap share), not done.

2026-09-30 Map walls shade objects from the sun (branch
`claude/project-thread-evqu3n`). Max (v0.1.6, Bedroom): the sun through
the window lands on baseplates with a stair-stepped edge, and a player's
shadow falls away from the sun while the bricks beside it read as lit only
by the lamp. Cause, from the code (no stock content in the cloud): the
map's surfaces took the sun from their baked visibility (lightmap texels,
filtered), but bricks, players, items and vehicles took it from the
visibility volume's sun channel, one binary ray per cell (2 units or
coarser, growing to fit 2M cells) read without filtering between
cells. So the patch's edge on bricks stepped in whole cells, and where a
cell said "no sun" beside a floor texel that had it, bricks lost the sun
and shaded only by the lamp while the player's live sun shadow still
landed on the sunlit floor. Now, in the Unified modes, the map's opaque
interior surfaces (the same set the volume traces) render into a third
depth layer per cascade that only objects read, with the same 3x3 filter
as lamp shadows. It reaches 10,000 units toward the sun (casters reach
400), since the map's walls stand far from the eye. Objects are sunlit
exactly where the walls beside them are; past the shadow distance the
volume stands in. The map's own look and Classic are unchanged, and there
is no protocol change. The layer costs one more depth layer per cascade
(+48 MB of shadow maps at High, +64 MB at Best) and a depth pass of the
map's interiors per cascade each frame. The test
`unified_lighting::map_walls_shade_objects_from_the_sun_with_a_filtered_edge`
(a roof 600 units up with a 4x4 opening, and a volume claiming sun
everywhere) has a floor that turns from sunlit to roofed within 0.8 units
and falls back to the volume without the layer; it passes on lavapipe. To
check on the PC: Cottage/Bedroom in Unified by the window and at the
dresser, and the 1M-brick frame cost.

Same branch, Max (Bedroom alarm clock): a hard, dark wedge fans across the
dresser from the clock's base. Inferred from the code (not rendered here):
the alarm clock is a map shape, which casts no lamp shadows, so the wedge is
a caster's (most likely the player's own body's) shadow from a recovered
light at the clock, the light the fit placed to explain the glow baked
around it. On a lightmapped texel a lamp shadow took away the lamp's whole
fitted share, capped only by the texel's static light. Beside the lights it
places, the fit overshoots (it can claim more light than the texel holds),
so the shadow took everything and went near black. Now a lamp takes its
proportion of all the fitted light there plus the mission ambient, which
always stays. `unified_lighting::lamp_shadows_on_the_map_take_only_the_lamps_share`
(fitted 0.55 against a baked 0.3) keeps 12 of 77 in the shadow where the
old rule left 0; it fails without the change. To check on the PC:
BedroomDark and Bedroom at the clock, first and third person.

Known gap (not planned for v0.1.7): the fit's lights that get no
visibility channel (7 on Bedroom, including small bright ones such as
reach 20, colour 1.0) light objects only through the baked residual and
never cast live shadows. The proper fix is to draw the map into each
casting lamp's cube faces for object receivers (static, drawn once per
lamp), which would also retire the 7-channel limit. Bricks cast lamp
shadows only with Brick Shadows on, on purpose: lamp shadows without sun
shadows would point bricks' and players' shadows different ways.

Same branch, Max (Bedroom, beside the desk lamp, High shadows): neither
the player nor a brick tower casts a shadow. The tower casts nothing
because Brick Shadows is off by default. For the player, inferred and not
rendered: High gives two lamp slots, picked by brightness and distance from
the eye, and the Bedroom fit has bright channelled lights (such as the bulb
inside the shade) whose light never reaches the dresser past the shade. The
picking never looked at the walls, so such lights could take both slots
while the desk lamp's light on the player cast nothing. Picking now weighs
each light by the share of the 27 visibility-volume cells around the eye
its light reaches (`shadow::tests` covers a hidden bright lamp giving up
its slot).
The Gate then measured that the two desk-lamp lights hold both slots at
Max's spot before and after, so that was not his cause. New evidence: the
instanced-caster path works on a lightmapped floor
(`unified_lighting::an_instanced_model_casts_a_lamp_shadow_on_the_map`, a
player-sized instance 12 units from a light 6 units up at High). Still open.
Leading hypothesis: the desk lamp's lower light (-11.8, 356.4, 195.0) sits
inside the lamp's own stem, and its upper light sits inside the shade. The
visibility volume's rays from the dresser to those lights are blocked by the
lamp's own geometry, so "seen" is 0 there: no direct lamp light on objects
and no lamp shadow on the map, even with the slot. The alternative is Max's
own settings (Lighting or Shadow Quality). `lighting_probe` now prints each
light's falloff, channel and volume verdict at points given in
`BRI_LIGHT_AT`, to settle it.
Settled with the Gate's probe at Max's spot (his prefs: Unified+Shine,
Best, Brick Shadows ON, so the Brick Shadows explanation above was wrong):
the volume cell there sees only channel 4 (light 9, 0.36, 6 units up); the
desk lamp's main light (light 0, 0.53, reach 140, 32 units up) reads seen 0.
The volume's 3.54-unit cells beside the lamp and dresser sit partly in the
geometry, so the volume hid the brightest lamp from everything on the
dresser: no light from it on the player or tower, and no shadow from it on
the dresser (a map receiver drops a lamp's shadow where the lamp's channel
reads unseen). Fix, as engines treat static geometry for shadowed lights:
each lamp slot keeps a coarse cube of the map's own surfaces (a
moving-caster-sized tile per face, one more layer at Best/High/Medium, drawn
once when a lamp takes its slot and when the map or its lighting changes),
and a slotted light's reach on both map and object receivers comes from it
(`lamp_reach` in scene.wgsl, normal offset 2-4 texels plus 1 toward the
lamp) instead of the volume, which stays for unslotted lights and past the
lamp shadow distance. Lamp picking keeps a quarter of its score for lamps
the volume hides near the eye. Test:
`unified_lighting::shadowed_lamps_reach_past_the_map_walls_not_the_coarse_volume`
(the volume hides the lamp where a slab shadows the floor and shows it
behind a map wall; fails without the map faces). Not rendered on Bedroom in
the cloud (no stock content); the Gate's Bedroom render at Max's spot is the
check.
Follow-up (Max 2026-09-30, after release): playtester pharzedia's faint
light strip across the Bedroom floor is baked in the v20 lightmap (live
lighting only subtracts from baked light). Max asked whether to clean such
leaks up long term. Proposed: at load, dim lightmap texels holding light that
no fitted light or the sun could reach past the map's real geometry, to
their surroundings; leave everything else as baked. Must be checked on every
stock map so it removes only leaks, never intended lighting.
Built for the release after v0.1.7 (Max 2026-09-30: "lets start working
on that in the release after which will include the portal and blockhead
ragdoll"). `map_lighting::Bake::leaks`: a lexel is a leak when it is a thin
ridge on its surface (brighter than the lexels 3 to both sides along one
lightmap axis, same normal, midway between them; by 8 levels of luminance
for authored light, 0.12 for baked sun share) and the geometry explains the
extra: for authored light the fitted lights would give at least 3/4 of it
more with no walls in the way (and it is 3/4 above what they give past the
walls); for baked sun, a ray to the sun is blocked. It takes the mean of
the two neighbours (sun share alike; the drawn lightmap loses the matching
sun part). Fixes (`MapLighting::leaks`, pairs for the drawn lightmap and its
decomposition) are cached with the bake (format 3; the key now covers the
drawn and decomposed lightmaps) and patched into the kept scene and its GPU
textures once the bake arrives (`GpuScene::patch_images`), in every mode.
Tests: `map_lighting::thin_light_leaks_through_sealed_walls_are_cleaned_up`
(a strip under a closed lit room goes; a bright trim the light reaches
stays), `thin_sun_leaks_under_a_closed_room_are_cleaned_up`,
`unified_lighting::patched_lightmaps_draw_without_a_new_upload`. Not yet
run on stock maps (no stock content in the cloud): `lighting_probe` prints
`Leak cleanup: image N (...): K texels` per lightmap and saves
`leaks-N.png` (changed texels red, local only, never committed);
`BRI_LEAKS=0` renders as baked for a before/after.
Gate probe at 83d780c7f: Bedroom 28 lightmaps / 642 texels, Kitchen 39 /
509; most isolated dots and short lines at lit-patch edges (real leaks),
but two regressions: speckles (a ring and a cross, 137 texels) inside the
window's sun patch on the Bedroom ceiling (base-lightmap-85), and a strip
through the Kitchen stove's orange glow panels (base-lightmap-63). Cause:
the ridge test compared raw brightness, so brighter detail inside a lit
patch the fit leaves unexplained or explains only in part counted as a leak
wherever some other fitted light was walled off. Rule now: the neighbours
must be explained by the lights past the walls (unexplained under 8
levels), the texel's unexplained light must carry the ridge, and the ridge
must outshine the neighbours' own light (a leak is light where there is
nearly none); baked sun needs neighbours under half the sun threshold and
the sun walled off from the texel and both neighbours. The light-leak test
now also has a broad lit patch crossed by brighter lines, left alone.
Max (v0.1.7, Bedroom dresser, Unified+Shine, Best, Brick Shadows on): his
player's and the tower's shadows point different ways. A cloud render of
the same arrangement (sun baked through a window over part of the floor, a
lamp to the other side, a kept tower and a moving player) shows why that
can be right: each object casts from both lights, but a live sun shadow
only takes away baked sun, so the tower standing outside the window's sun
patch shows only its lamp shadow while the player inside it shows its sun
shadow. `lighting_probe` gained `BRI_TOWER`, `BRI_PLAYER`, `BRI_LAMPS=0`,
`BRI_SUN=0` and `BRI_LIGHT_SCALE=k` to show which light casts which shadow.
Gate renders (RTX 4070 SUPER, Vulkan; tower at -22,348.5,188, player on the
dresser at -16,348.5,196, eye -6,362,222 toward -30,351,193): the first
read looked like the tower's sun shadow vanishing whenever lamp shadows
were on, but that compared lamps-on against `BRI_LAMPS=0`, which also
swaps the lamps' visibility from the map faces back to the coarse volume.
Split at half light, the four sampled pixels are identical with or without
the stand-ins and with or without the sun in both lamp modes: no sun
reaches them, and the difference was lamp visibility only. In view, both
stand-ins cast lamp shadows away from the desk lamp (tower on the wall and
window frame, player a streak on the dresser); no sun shadow is missing.
A bisect of the shadow passes (one-by-one draws instead of multi-draw,
skipping each kind of lamp tile) also found nothing wrong. Concluded:
correct behaviour, no renderer change.
- 2026-09-30 Painted brick emitters keep their authored alpha (branch
  `claude/ice-palace-particles`). Max (v0.1.4): Slate "Ice Palace.bls" drew
  its fog as opaque white clouds burying the map. The save has 152 Fog A and
  47 Fog B emitters (also Fire A/B, Laser A, Water A, Player Bubbles), almost
  all on opaque white (palette 15) bricks. The converter is faithful:
  `FogParticleA` peaks at alpha 0.5 and `FogParticle` at 0.1, both fading
  from and to 0, and both emitters have `useEmitterColors`. Cause:
  `fx-runtime::brick_source` passed the paint as all four emitter colour
  keys, so paint alpha 1 replaced the fade and every puff drew at full
  opacity. v20 feeds a brick emitter one colour through
  `ParticleEmitterNode::setColor(getColorIDTable(colorID))` (decompiled
  `fxDTSBrickData::onColorChange` and the emitter plant path); the engine
  side is closed source. The runtime now treats it like the spray-can
  recolour: a new `SourceOptions::paint` (gated by `useEmitterColors`)
  replaces the RGB on every key and keeps the authored alpha keys, also for
  live particles when the brick is repainted. That alpha stays authored is
  inferred from the authored 0 to 0.5 to 0 fades (an alpha-1 override makes
  them pointless and Ice Palace a whiteout) and matches the save's own v20
  thumbnail (`saves/Slate/Ice Palace.jpg`: thin wisps round the palace).
  Emitters without `useEmitterColors` (fire, jets) never took paint and are
  unchanged. Client-only; no protocol or content-pack change. Tests:
  `bri-fx-runtime --test runtime brick_paint_tints_rgb_but_keeps_authored_alpha_keys`
  and the content-backed (ignored) `original_painted_brick_emitters_never_exceed_authored_alpha`,
  which starts every `useEmitterColors` emitter in effects-runtime-pack-005
  on an opaque white brick; both failed before the fix and pass after.
## 2026-09-29 Vehicle camera and vehicle move as one; Tank steering agrees with the host (branch `claude/vehicle-camera-sync`)
Max, v0.1.4: the camera and the vehicle jerked apart on quick moves (horse
turning in third person, the stunt plane pitching, the Magic Carpet in
third person), and the Tank's mouse and A/D steering seemed to fight.

Causes found (audit rows 56 to 60 in `docs/audits/vehicles.md`):
- The host ran a seated player's moves in a way the client could not
  replay: a late move repeated the last one, and past a backlog of six it
  ran three moves in one step. Uneven client frames alone starve a
  one-a-tick queue, so the driver's view was corrected every second or so
  (51 visible corrections in 6 s on the carpet in the new test, worst 1.4
  units). New `SeatedPace`: one move a tick; a late move leaves the queue
  one longer, which then absorbs jitter of that size; only a queue that
  stayed above two spare moves for two seconds drains, one extra move at
  a time (a stall's backlog of 30+ still drains three at a time).
- A correction carried only the newest predicted tick's jump, but the
  vehicle is drawn between two ticks, so part of every correction popped
  on screen (0.38 units on the horse). Now the whole drawn jump is carried
  and eased out, no faster than 4 units/s and 1 rad/s, so the rigid chase
  camera never whips.
- The horse's camera turned by the raw mouse while the horse turned on its
  predicted ticks: the view led the horse. `Controls::mount_look` takes the
  drawn mount's heading plus the head's free look (v20 builds the view from
  the control object's render transform), in first and third person.
- The host assumed stock v20's steering prefs (strafe steering on) until
  the client's `SteeringPrefs` arrived, and a map change (`Session::adopt`)
  dropped them, while the client predicted with its own (off by default).
  Then the host steered the Tank by A/D and the client by the mouse. Now
  the host assumes the shipped prefs (`DEFAULT_STEERING`), carries them
  across maps and forgets them on leaving; every `VehiclePose` echoes the
  prefs the host steers that driver by (`driver_steering`), and the client
  predicts and picks its seat role by the echo; the client also resends
  its prefs on every seat change. Protocol change: `VehiclePose` gains
  `driver_steering` and `steering_quiet`.
- With auto-return on, the steering returned on every 120 Hz move without
  mouse yaw, which is most moves while the mouse moves at the frame rate;
  v20's moves are 32 ms. Mouse steering now returns only after 4 quiet
  ticks (one v20 move; `steering_quiet`, restored for prediction and saved
  in checkpoints); strafe keys unchanged. In v20 (and here) the Tank
  follows the Jeep's rule: prefs off, the mouse steers and A/D do nothing;
  on, A/D steer and the mouse turns the head.
- Not fixed, by decision: the Magic Carpet's hull scraping the ground (it
  hovers 0.8 units clear, so pitching past about 17 degrees scrapes) does
  not replay step for step through Rapier's contact history (warm starts,
  recycled manifolds, the refresh step); in the air it predicts to 3e-5.
  Removing warm starts or the refresh step made it worse. The eased
  correction above covers what it leaves.

Evidence: new `motion::tests::a_driven_vehicle_is_drawn_smoothly_through_corrections`
(host world with the session's move pace, jittered moves and poses,
6 to 25 ms frames; carpet, Flying Wheeled Jeep, horse, Jeep, Tank): fails on
main's logic (51 visible corrections, worst 1.42 units; horse pops 0.38),
passes here (at most 1 visible correction after 2 s, no pops, ease within
the caps). `controls::tests::a_mount_rider_looks_along_the_drawn_mount`,
`app::tests::a_driver_is_predicted_with_the_hosts_steering_prefs`,
`steering_prefs::auto_return_waits_a_whole_move_before_fighting_the_mouse`
(fails with the old rule: 0.66 vs 0.75 rad),
`vehicles::the_host_steers_a_driver_by_the_prefs_it_echoes` (no prefs sent,
explicit prefs, seat change, map change, reconnect), and
`vehicles::predicted_vehicles_stay_uncorrected_under_real_timing` (real
session host: Jeep and Flying Wheeled Jeep 0 visible corrections).
`riders_keep_their_look_on_every_mount` updated: with the shipped prefs the
Jeep's and Tank's driver is mouse-steered.

## 2026-09-30: Player Appearance decal thumbnails

Max reported every tile in the shirt (Decal) picker, and the Decal slot
itself, showing the NONE icon while the avatar preview drew the shirt fine.
Cause: the UI importer stores only v20's 64x64 `thumbs/` images for faces
and decals (the full textures live in the avatar pack), but the picker's
`icon()` looked up thumbnails for faces only and asked decals for the full
image, which the UI pack never has, so every decal fell through to NONE.
Fix: faces and decals both use their `thumbs/` image (as v20's
allClientGuis does, e.g. `Add-Ons/Decal_Default/thumbs/Medieval-Tunic`),
then the full image, then NONE. The importer now also packs the full image
for a face/decal add-on that ships no thumbnail, so those show a picture
too. Client-only; no protocol change.

Evidence: new `screens::avatar::tests::face_and_decal_pickers_show_thumbnails`
(fails on the old lookup for the decal thumbnail case); `bri-ui` avatar
tests, `bri-ui-import` tests and clippy on both crates pass.

- 2026-09-30 v20 jump timing: bunny hops and ramp launches keep their speed.
  Max: jumping forward off a ramp and hopping on should carry momentum, as in
  v20. From a read-only disassembly of blocklandv20.exe (raw bytes checked
  with capstone): jumpDelay runs down every tick, in the air too (updateMove
  0x5AFAC3); a floor hit in updatePos (normal.y > 0.8, 0x5B175B) reopens the
  jump at once; canJump (0x5A2AA0) refuses after a ceiling hit
  (`JumpState::ceiling`, 0x8A2) and its rising guard uses horizontal speed;
  above maxJumpSpeed the tick's jump bookkeeping is skipped; air jumps push
  along air control's rewritten move. A held hop now rejumps the tick after
  landing (was 4 ticks later), so each landing costs about 3 u/s instead of
  all speed above run: 15 u/s hops keep 13.4, 10.4, 7.6; down a 45 degree
  ramp the launch reaches 19 and hops keep 17, 14, 11, 8. The pine tree's
  69.6 degree faces launch 11.5 u/s facing away. Protocol change:
  `JumpState` gains `ceiling`. Tests: sim `player` (bunny hop, steep face,
  rehop timing), `jump_edges`; motor, sim, net suites; clippy on motor/sim.
- 2026-09-30 Jump stops working after walking into a brick wall (branch
  `claude/jump-stuck-3j3efr`). A joiner on Max's server: "sometimes i cant
  jump ... i have to like jet or crouch and then i can jump again". Cause:
  the v20 ceiling rule added with the jump timing port (`JumpState::ceiling`)
  counted any downward polygon in a blocking hit's list. Walking into a wall
  of stacked bricks grazes the upper brick's underside edge-on at the seam
  (an edge contact, `face_dot` 0), so the flag latched; walking and standing
  on level ground make no further blocking hit, so jump stayed refused until
  a jet's landing (or a crouched wall hit below the seam) cleared it. Fix
  (`torque::update_local`): only a downward polygon the box's top meets
  head-on is a ceiling; a real head bump under a lintel still counts. No
  protocol change. Test: sim `player`
  `walking_into_a_stacked_brick_wall_keeps_the_jump` (fails without the fix
  for seams at 0.6, 1.2, 1.8 and 2.4) and
  `wandering_a_ramp_brick_roof_never_latches_a_ceiling` (the second report
  was on a roof, where crouching did not help and jetting did; a seeded walk
  over a 45 degree ramp-brick roof latches without the fix) and
  `crouching_into_a_brick_corner_keeps_the_jump` (Max reproduced it by
  crouching into a corner; 400 seeded brick corners, 40 lose the jump
  without the fix, none with it); motor and sim
  suites and clippy pass
  (content-needing `tools` tests not run in the cloud).
## 2026-09-30 Color Warning on every load

Loading a build asked "Color Warning" even with the same colour set. The
check (`saves::color_difference`) and the Add More Colors merge
(`LoadMapping`) compared colours by exact float equality, while v20 saves
hold colours rounded to 8 bits with six decimals (the default set's 0.900
red saves as 0.898039). v20's `LoadBricks_GetColorDifference` uses
`colorMatch`: every RGBA component within 0.005 of any slot. Both paths now
use `bri_world::build::color_match` / `merge_palette`; saved slots with alpha
under 0.0001 (v20's `1 0 1 0` filler) are never appended. Read-only check on
the PC: all 35 stock v20 saves pass v20's own check against the reference
colorSet.txt. Test: `saves::tests::a_save_of_the_same_colorset_loads_without_asking`.
Client and host both change; no wire protocol change.

## 2026-09-30 Startup shows the menu at once (branch `claude/faster-startup-vv3rld`)

Max: the standalone exe took about 5 s to start and showed a big white
rectangle meanwhile. Measured on Linux with the v0.1.6 release content and a
software GPU (llvmpipe, Xvfb): content loaded at 295 ms, window created at
309 ms, GPU opened at 449 ms, then `gpu_ready` compiled pipelines until
3576 ms (avatar preview's scene renderer 1.6 s, the world's 1.5 s: twelve
variants of the big scene shader each) before the first frame at 3588 ms.
The window existed and was empty (white on Windows) for those 3.3 s.

Now:
- The window is created hidden and shown only after its first frame is
  drawn and finished on the GPU (`platform.rs` `resumed`, which draws that
  frame itself because Windows sends no redraw to a hidden window).
- The world's and the avatar preview's `SceneRenderer`s compile on worker
  threads (`gpu_build::Building`); menus draw meanwhile. Drawing the world
  waits for them (`wait`), the avatar preview renders once they are ready
  (`ready`). Settings changes that rebuild them work as before.
- Each phase is logged: `Startup: content loaded / window created / GPU
  opened / first frame drawn / window shown at N ms` and `Compiled scene
  pipelines in N ms`, so a slow start on a player's PC names its cause.
- The standalone launcher's first run of a new version unpacks with up to
  8 threads while another hashes the payload (damage is still refused and
  the staging folder removed). v0.1.6 Linux payload, 2979 files, 4 cores:
  0.45-0.9 s, was 1.7-2.0 s.

Same machine after: window shown with the main menu at 375 ms (was 3588 ms);
screenshots at 0.2-4 s confirm the menu from 0.4 s. On DX12 wgpu uses FXC
(no dxcompiler.dll shipped) and caches compiled shaders per process; the log
lines above give the real numbers on Windows. No wire protocol change.
## 2026-09-30 Airborne horse and Stunt Plane first-person stutter (branch `claude/vehicle-airborne-stutter`)
Max, v0.1.6: turning quickly on a horse standing still was smooth in third
person, but jumping around while turning stuttered; the Stunt Plane
stuttered the same way in first person (not third).

Two causes, not shared, with one theme (v20's 32 ms tick state shown
without v20's between-tick easing):
- The horse (any player-type mount) moves on the player motor's 32 ms
  Torque ticks. Players are drawn between their last two ticks
  (`PlayerState::shown_feet`), but a mount's vehicle transform is its
  physics body, which sits at the tick's feet, so the drawn horse and the
  third-person camera riding it stepped every fourth 120 Hz tick whenever
  it moved (a jump most visibly; turning in place does not move it). Now
  `VehicleSnapshot::shown_transform` draws a mount between its ticks, for
  the rider's prediction (`Drive::body`) and the poses others see; the
  body stays at the tick's feet for physics.
- A mouse driver's head took each mouse move's pitch at once and sprang
  back continuously, tipping the first-person view by the whole move.
  blocklandv20.exe adds the move's pitch to `mHead.x` on its 32 ms tick
  (0x5aea0c) and halves it in first person on the same tick (0x5aeb0b);
  the view eases between ticks. `Controls` now runs the driver's head on
  those ticks (`HeadTicks`): a flick eases in at half its size and out.
  The third-person chase camera never used the head's pitch, which is why
  only first person showed it.

Ruled out: the Stunt Plane's drawn pose (predicted, smooth: largest
frame-to-frame change of velocity 4 units/s, of spin 0.05 rad/s); the
angle decomposition of a rolled first-person view near vertical (exact to
about 1e-3 rad); the mount's motor state over the network (every field of
`PlayerState`, jump bookkeeping included, is serialized).

Evidence: `motion::tests::a_driven_vehicle_is_drawn_smoothly_through_corrections`
now includes the Stunt Plane and a horse jumping as it turns, and counts
frames whose drawn velocity changes by more than 8 units/s: with the
mount drawn at its tick's feet (main) the horse has 280 such frames (worst
70 units/s); now 4 to 5 (take-offs and landings). New
`controls::tests::a_mouse_drivers_view_tips_as_smoothly_as_v20s`: main's
controls tip a 0.3 rad flick to 0.26 rad at once; now it peaks at 0.15
eased over a tick. `a_mouse_driver_steers_and_free_look_springs_back_in_first_person`
updated to v20's tick timing.
Tank (same branch, follow-up): Max found the Tank's steering clunky, "the
whole rear of the tank begins to turn". The Tank's four-wheel steering
already matched v20 (`TankVehicle::onAdd`: wheels 0/1 x1, 2/3 x-0.8, all
powered), and its mouse/A-D rule is the Jeep's (strafe steering off, the
default: the mouse steers, A/D do nothing; on: A/D steer, the mouse looks).
The difference was the wheel angle: blocklandv20.exe squares the steering
before turning the wheels (`updateForces` 0x5746ea: fld mSteering.x, fabs,
fmul, fchs, fsin/fcos), so a small mouse turn steers gently; ours turned
the wheels by the steering itself. `Wheel::steer_angle` now turns each
wheel as Torque does (physics and the drawn wheels). A model of Torque's
tyre forces for v20's Tank (`tools/tge_tank_turning.py`, from Torque's
`WheeledVehicle::updateForces` and Vehicle_Tank.cs) circles in 28.4 at a
quarter turn and 9.0 at half; ours did 8.0 and 5.3, now 28.3 and 8.0. Open:
at full lock ours circled in 12 against the model's 3.8, because Rapier's
wheels grip sideways almost rigidly where Torque's tyres are springs in a
friction circle (audit row 64; fixed below). Evidence:
`schema::tests::wheels_steer_by_the_squared_steering`.

Tank full lock (same branch, follow-up): Rapier's ray-cast vehicle
controller is gone. `crates/vehicles/src/world/tires.rs` ports Torque's
`WheeledVehicle::extendWheels` and the wheel half of `updateForces`, which
blocklandv20.exe keeps: a ray from each hub mount down spring plus tyre;
the spring `force x (1 - extension)`, a damper on compression only, the
anti-sway push from the opposite wheel and the bottom-out impulse; each
tyre a spring sideways and lengthways (`lateralForce`/`Damping`/
`Relaxation`, longitudinal likewise) held inside a friction circle of the
wheel's load times `staticFriction`, or `kineticFriction` once slipping;
wheel spin from `engineTorque` (less toward `maxWheelSpeed`, doubled while
jetting forward), the tyre's pull, `brakeTorque` or `engineBrake`. All
wheeled vehicles and skis (whose NothingTire grips nothing) use it. Every
wheeled vehicle now also takes v20's drag (`drag` on velocity, `rotationalDrag
+ drag` on spin, skis audit row 6); only the flying ones had it.
Schema 7 gives each wheel its tyre and the spring's `antiSwayForce`
(vehicles-pack-012; the schema 5 upgrade is gone). Checkpoint schema 3
saves each wheel's extension, contact, rotation, spin and tyre stretch.
`VehiclePose::wheel_tire` carries each wheel's spin and tyre stretch, so a
driving client's replay starts where the host's tyres are: a wire change.
Evidence (content-free): `tires::tests::a_tank_like_vehicle_turns_as_torques_tyres_do`
drives a vehicle with v20's Tank drivetrain and tyres against a
two-dimensional Torque model in the test: circles 28.4, 9.1, 3.9 at 0.25,
0.5 and full lock against the model's 28.4, 9.2, 3.9; Rapier's wheels gave
28.2, 8.0 and 23.5. `a_replay_from_the_hosts_pose_matches_the_host`: a
replay from a full-lock pose lands within 0.001 of the host after one
second (0.51 away without the tyre state). With content:
`tires::tests::the_tank_turns_as_torques_tyres_do` (replaces
`the_tank_circles_as_v20s_at_part_lock` and `tools/tge_tank_turning.py`).
The Stunt Plane's committed pack needs a re-import
(`python tools/default_addons.py import`) for schema 7.
friction circle (audit row 64). Evidence: `steering_prefs::the_tank_circles_as_v20s_at_part_lock`
(fails on main: 8.0 at a quarter turn), `schema::tests::wheels_steer_by_the_squared_steering`.

## 2026-09-30 Mirrors in mirrors (branch `claude/project-thread-vvxj2v`)

Max's v0.1.6 playtest: mirrors "work basically perfect", but two mirrors
facing each other looked buggy: a mirror seen in another's reflection was
flat silver. Now `reflection::plan` plans a tree of views: planes the
player sees and planes seen inside a live plane's reflected view compete
for the Mirrors setting's passes by the screen they fill (Low 1, Medium 2,
High 3 passes, unchanged, so the worst-case cost is too). A nested plane's
view is its parent's clip matrix reflected in its plane, clipped at it
and cropped to its pixels in the parent's viewport, rendered before its
parent. A mirror seen deeper than the passes reach is an echo: it shows the
nearest live plane of its wall's last picture, reprojected through the
view that plane was seen in (clamped to the part it drew), so facing
mirrors repeat into a tunnel a frame at a time, with no extra pass. The
Mirror Add-On's glass reflects 95% (`tint`), so each bounce dims. Kept
reflection textures a plane no longer uses now draw silver (they used to
keep showing their last picture when the live plane count dropped).
Tests: `reflection::tests::facing_mirrors_show_each_other_a_bounce_deeper_within_the_passes`
(texel-exact double reflection, billboard turning, Low stays silver);
`-p bri-render --test mirrors`
`facing_mirrors_show_what_only_the_one_behind_the_viewer_sees` (a card
seen only via both mirrors, on its own side; Low shows none) and
`beyond_the_passes_facing_mirrors_repeat_what_the_nearer_mirror_showed`
(a second frame adds the card's echo deep in the tunnel). Client-only; no
protocol change.

## 2026-09-30 Gravity Gun rework (branch `claude/gravity-gun-rework-ainb6j`)

Max: "we really fucked up its behavior", and it used the Rocket Launcher.
A headless probe of the old gun measured why it felt bad: held crates
overshot the hold point by 0.6 units and a Steel Ball swung 1.1 either
side; turning at 180°/s left things 1.1 to 1.9 units behind the aim;
looking down with a heavy ball shoved the holder off their feet and broke
the hold; a throw only came from a charged release. He asked for Garry's
Mod's physics gun: grab where you point, smooth drag and swing, fling by
flicking your view, the wheel to reel, a bending beam.

The engine hold (`session/movables.rs`) is rewritten as a critically damped
velocity servo: it holds the grabbed spot (`at`), not the middle; it
carries the thing at the aim point's own velocity plus a closing speed
limited so it stops without overshoot (`min(gap·18, √(1.6·a·gap), 60)`),
with acceleration capped by `force / mass`; with `turn` it keeps its angle
to the holder (yaw-rate feed-forward, spin limited by accel/radius); the
point is kept clear of the holder's own body, and a hold ends when the
holder stands on what they hold, or it snags. Held players tumble (the
deathvehicle body) so they are not fought over by client prediction;
corpses can be held (movable by those who could move the player when they
died). Vehicles held never time out of a tumble. New ops:
`hold(p, ref, d, #{at, force, turn})`, `hold_distance(p, d)`; reach 64.
Images gain `commands.wheel`: while the trigger is held the mouse wheel
sends that command its notches instead of scrolling the inventory
(`UiUpdate::ToolWheel`, `GameAction::ToolWheel`). Right click jets again
(the blast is gone: it clashed with jetting).

The tool is the stock Printer by reference (`base/data/shapes/printGun.dts`,
never committed). Client code gets two generic `world.read` functions:
`held(player, hand, out)` (where the image is drawn this frame and its
muzzle) and `image_mesh(kind)` (a held image's own model as an Add-On mesh,
built client-side once per model). The effects use them: the beam leaves
the drawn Printer's muzzle along the aim and bends into the grip (a
quadratic curve through a pull on the aim line), and an original alien
skin (`alien.wgsl`: dark oily shell, cold thin-film sheen, veins that
pulse and flare while the beam is on) is drawn over the Printer, puffed
out a hair so it covers it.

Evidence: `-p bri-sim --test showcase` (14: grabs where it points and
trails < 0.45 at 180°/s, settles without wobble light and heavy, flick
flings, wheel reels, looking down sets it before you, a flung vehicle
kills and credits the thrower, trust decides outside minigames and a held
player stays limp, corpses carried and dropped, right click jets);
`-p bri-net --test showcase` (a second player sees a lift and drop);
`-p bri-client-sandbox --test showcase -- --include-ignored` (effects
follow the state, the beam starts at the drawn muzzle, the skin is drawn
at the gun's matrix; offscreen render on llvmpipe);
`-p bri-ui --test runtime_input wheel_goes_to_the_held_tool...`. Protocol
unchanged (package command arguments already existed). With the Ragdoll
Add-On (merged from `claude/blockhead-ragdoll-ee3dyw`), the effects also
grip the ragdoll limb the beam met (`rigid_find`) and pull it to the
beam's end each frame (`rigid_hold`), so a carried corpse dangles from
that limb and flies on when let go; the server still carries the corpse
(`a_ragdoll_dangles_from_the_limb_the_beam_grabbed`). Max asked for
admins to grab live players: outside minigames an administrator may now
move anyone and anything (as they may already fetch and teleport
players); inside a minigame its rules decide for them too
(`an_administrator_can_grab_anyone_outside_minigames_but_not_inside`).
Needs the PC:
the Printer image's offset and rotation against v20's `printGunImage`, the
beam leaving `printGun.dts`'s muzzle in first and third person, and a
look at the skin on the real model.
## 2026-09-30 Blockhead ragdoll Add-On (branch `claude/blockhead-ragdoll-ee3dyw`)

Max asked for a "funny blockhead ragdoll" in place of the death animation,
as an Add-On you enable, and for the Gravity Gun to toss ragdolls when both
are on. Built from two generic client-sandbox capabilities, no ragdoll code
in the engine:

- `physics.local` (sandboxed): rigid bodies (box, ball, capsule), ball
  joints with swing/twist limits and friction, push, get, and shared bodies
  any Add-On can `rigid_find` along a ray, push and `rigid_hold` (a spring
  to a moving target that cancels gravity). The game simulates them in a
  per-Add-On Rapier world (`crates/client/src/addon_physics.rs`) that shares
  `local_physics` with brick debris: surroundings made solid near bodies,
  players and vehicles as one-way pushers, shots striking. Budgets: 256
  bodies, 512 joints, 1,024 calls a frame, 4 ms of simulation a frame (30
  strikes stop it). Commands apply after the frame; the next frame reads
  the result.
- `avatar.pose` (sandboxed): `skeleton` reads a player's drawn nodes with
  the bounds of what is drawn on each, `skeleton_part` finds the node a
  body part is drawn on, `pose` places nodes (children follow). The eye
  node is never posed, so view and aim are unchanged.

`packages/showcase/ragdoll` (hand-written WAT, CC0) builds one box per body
part from the drawn bounds, joins each to its nearest posed ancestor with
limits per part, starts it at the corpse's velocity plus a random pop and
spin, and passes blasts on the corpse to every limb (it no longer pulls a
ragdoll back towards its corpse; see "Ragdolls slide down ramps"). Its bodies are shared, so a Gravity Gun (or
any Add-On) can pick them up.

Shipping: `packages/default-addons.json` entries take `"enabled": false`.
Such an Add-On is installed into `content/addons/<id>` (checkouts and all
three release packagers) but never listed, so the Add-Ons screen finds it
off and "Default" leaves it off. `PackageInfo::side` now makes an Add-On
with only client code `client` ("Just you"), as the packagers already did.

Tests: `-p bri-client-sandbox --test ragdoll` (build, joints, blast),
`bri-client` `addon_physics::tests` (fall, joint, hold, shove, and the real
module falling in one piece and settling on a floor), `bri-package`
`defaults::tests` (the Ragdoll installed off, side client). Client-only; no
protocol change. Not verified here (no content in the cloud): the real
Blockhead rig's part-to-node mapping, the look and feel, and frame cost.
## 2026-09-30 Stunt Plane first person: the view no longer leads the plane (branch `claude/project-thread-zai0o2`)

Max's v0.1.7 playtest: pulling the Stunt Plane up or pushing it down in
first person, "my camera moves first and then the plane takes a second to
catch up"; in third person the pilot's body nodded with the mouse. Cause:
a mouse driver's head took each mouse move's pitch (blocklandv20.exe
0x5b2cd4, halved back each tick in first person, never in third), so the
first-person view tipped toward the new heading a tick before the plane's
steering torque had turned the nose, then sprang back while the plane
caught up; in third person the head kept the pitch and posed the body.

Fix: `Controls::look` gives a mouse driver's pitch to the steering only;
the head stays on the seat, as Torque's `Player::processTick` hands a
controlling rider a null move. The first-person view is now the seat's
rotation, rigid with the drawn plane, and the pilot no longer nods in
third person (others already saw a level pilot: the host keeps a mouse
driver's body pitch at 0). Free Look still moves the head and springs
back in first person. This departs from the v20 exe on purpose, recorded
in `docs/audits/vehicles-v20-checklist.md`; Max remembers v20's view as
steadier than ours. `HeadTicks` now only eases a Free Look return.
Evidence: `controls::tests::a_mouse_drivers_view_never_leads_the_vehicle`
(144 frames of mouse flicks, first and third person: view within 1e-6 rad
of the seat, body pitch 0, steering moved). Client-only; no protocol change.
## 2026-09-30 Portal bricks: linked bricks you see and walk through (branch `claude/portal-bricks-5be9t8`)

Max asked for Portal bricks on the window model, and how two know they
belong together. Pairing reuses v20's brick Name (wrench) like Teledoors:
bricks of one linking definition, one owner and one name (case-insensitive)
are a pair; more form a ring in brick-id order; two placed in a row get a
matching `Portal_xxxxx` name (special.rs's teledoor naming, generalised).
Pairing is a pure function of the replicated world, so the protocol is
unchanged.

Engine seams (generic, no portal code): `Link` on a catalog entry (JSON
`link`, `.cs` `link*` fields); `bri_content::passage` (openings with a rigid
carry); `bri_sim::links::Links` (pairs, sides, passages; host, prediction
and the client's view index keep one each); the motor soup cuts what lies
behind an opening and fills it with the partner's side, so a portal set
against a wall walks through it; unpaired passable openings are panes.
Players are carried by the middle of the body crossing (velocity, yaw,
prediction's pending inputs and the camera all turn with it); vehicles by
their centre (host, driven prediction, remote interpolation); projectiles
and dropped items by their sweep (`Query::passage`). The mirror renderer
takes any rigid transfer (`Looks::Through`), so views share Mirrors
Low/Medium/High and the echo for portal-in-portal; a window the eye is
about to cross draws recessed so it never clips.

Add-On: `packages/brick_portal` ("1x4x5 Portal", Special > Portals), on the
window mesh by reference (no v20 content), listed `"enabled": false`.

Evidence: `bri-sim --test portals` (pairing, renaming, rings, walking
through turned with steps under 0.3 and no sideways drift seen from the
entry side, a wall behind the doorway, an unpaired doorway shut);
`bri-content passage` tests; `bri-render reflection` tests (unflipped
window view, recessed window); `bri-weapons --test runtime` (a thrown item
through a portal, turned, same speed); `bri-convert catalog` (`link*`
fields); `bri-package defaults` (installed off). Not verified here (no
content or GPU in the cloud): the look and frame cost.

Frame fit (Gate measured the real `4x1x5window.blb`: front opening 1.8 x
2.64 with a 0.28 sill, inner tunnel 1.9 x 2.75 with a 0.2 sill; the player
is 2.65 tall): `frame` now takes per-edge widths (`{sides, top, bottom}`,
`.cs` `linkFrame="sides top bottom"`), and the Portal uses the tunnel's,
0.05 / 0.05 / 0.2, an opening 1.9 x 2.75. A standing player steps onto the
0.2 sill and walks through (`portals` test asserts the rise); a uniform
frame covering the sill would have left 2.44, too low to stand through.
Wall portals (one open side) now turn half about the upright, not the
side's first in-plane axis, which had flipped south-facing ones upside
down (`bri-content` brick test).

Limits: vehicles use rapier collision, so walls right behind a portal still
stop them, and an unpaired portal's pane stops only players; the
third-person camera sweep is not portal-aware; a body half through shows
only on the side it has not crossed yet (its front half is hidden for a moment); a projectile shows past a portal for up
to one host update before the host's correction (no extra network).

Follow-up: a ragdoll belongs to the life it died in, not the alive flag.
`world.read` gained `life(player)` (`Vitals::spawn_tick`, from the
respawn-pose fix merged in); the ragdoll lets go when it changes, since the
corpse and the respawned body share an owner id. Both `life` and the
`players()` alive flag follow `avatar::drawn_life` (the body and death as of
the drawn pose), so the ragdoll starts and lets go exactly when the drawn
body dies and changes. The real-content check is
`cargo test -p bri-client --lib ragdoll_on_the_real_blockhead -- --ignored --nocapture`.
## 2026-09-30 Lamp shadows follow building at once and are sharper (branch `claude/project-thread-evqu3n`)
Playtester pharzedia (video via Max, an older build): shadows lag and look
pixelated in the Bedroom. From the video: a brick's lamp shadow appeared
about a second after it was placed, a removed brick's shadow stayed on the
wall, the player's lamp shadow on the city floor was a blob, and lamp
shadows far from the ceiling light were blocky. Both are in v0.1.8 too.
Causes: kept brick lamp faces were redrawn only when their lamp or view
changed, plus one face a frame in turn (24 faces at Best, so up to 24
frames late), and at Best a face was 512 texels (256 for players,
vehicles and items) across 90 degrees, so tens of units from a Bedroom
lamp a texel is a large fraction of a unit. Fix: each kept face remembers
which static chunks it was drawn from (the chunks inside its frustum
within the lamp's reach, identified by their pooled geometry, which a
rebuilt chunk never keeps) and is redrawn the frame that set changes; the
turn-by-turn refresh stays as a backstop. Best's lamp faces are now 1024
(moving casters and the map faces 512), 10 extra layers of the shadow
array instead of 4 (about 96 MB more video memory at Best; High and Medium
unchanged). The interior lights are real-time lights: bricks never edit
the map's lightmaps; lamps add live-shadowed light over the baked map.
Test: `bri-render --test unified_lighting
placed_and_removed_bricks_change_lamp_shadows_the_same_frame` toggles a
brick chunk under a lamp every frame; it failed on frame 1 before the fix
(removed slab still shading, 25 vs 154) and passes after. Client-only; no
protocol change. For the Gate: the 1M-brick frame cost at Best should be
checked against the perf headline (kept faces redraw at 4x the texels).
## 2026-09-30 — A respawned player no longer gets up from the death pose

Max: after dying and respawning, the new body started in the death
animation and quickly stood up. Two causes. (1) The corpse and the respawned
body share the owner id, so the client kept one `AvatarMesh` and blended out
of `death1` over `sAnimationTransitionTime` like any action change; v20's
`GameConnection::spawnPlayer` makes a new `Player` whose threads start at
`root`. (2) The avatar's `dead` came from the newest vitals, which travel on
the 6-tick update stream, while poses are datagrams every 3 ticks: a
client's own new body at the spawn point was drawn before its vitals said it
lived, so it lay in `death1` there and then got up. Remotes, drawn about 9
ticks behind, stood up where they died before the respawn teleport reached
them.

Fix: `Vitals` carries `spawn_tick` and `died_tick`, and a client's own
`Pose` carries its body's `spawn_tick` (remote poses do not; they are drawn
behind the vitals). `avatar::drawn_life` decides the drawn body and whether
it is dead at the drawn pose's own tick, so death and respawn land where the
pose timeline has them, as v20 replicates damage state with the object.
`AvatarMesh::set_body` drops every running thread (action, transition,
crouch) for a new body, and the client forgets that owner's thread-2/3
actions. The owner's own stream sends a new body at once even where the old
one stood. Tests: `avatar::tests::death_and_respawn_follow_the_drawn_poses_timeline`,
`stream::tests::a_new_body_reaches_its_owner_at_once_even_where_the_old_one_stood`,
content `avatar::tests::a_respawned_body_stands_in_root_without_getting_up_from_the_corpse`
(the first attempt compared only the `Eye` node, which on the real
Blockhead did not differ between the corpse and the standing body; it now
compares every posed node and first checks that `death1` moves the body).
Protocol change: `Vitals` +2 fields, own `Pose` +1 (Gate assigns the number).
## 2026-09-30 The GPU opens while the content loads (branch `claude/faster-startup-vv3rld`)

Max's v0.1.7 logs on DX12: content loaded 764-852 ms, then the GPU opened
at 1525-2095 ms (device plus the menu renderer's shaders, 750-1200 ms), menu
shown at 1579-2194 ms; scene pipelines compiled in the background in
3.2-4.3 s. The two waits ran one after the other.

`platform::EarlyGpu::start` (called first in `main.rs` `run`) now opens the
first backend `open_gpu` would try, and builds the `UiRenderer`, on a worker
thread while `App::load` runs. `Graphics::new` makes the window's surface from
that instance and uses it if the adapter can present to the window; otherwise
(or if the early open failed) it opens the GPU the usual way with the same
fallbacks. Not on macOS (GPU objects stay on the main thread there). A lost
GPU reopens the usual way.

Linux, llvmpipe, v0.1.7 content, `WGPU_BACKEND=vulkan` so the early path is
taken: "GPU opened" 2-3 ms after "window created" (was about 45 ms); menu
shown and drawn as before (screenshots 0.2-4 s). The default backend order on
Linux tries DX12/Metal first, finds none and falls back as before. Expected on
Max's PC: menu about 0.75 s sooner. No wire protocol change.
## 2026-09-30 Advanced Duplicator (branch `claude/advanced-duplicator-xemi1d`)

Max's call (after lpsroo and Wilfred asked): keep both duplicators. The
classic Duplicator stays on and unchanged; the Advanced Duplicator ships
installed but off (`packages/advanced-duplicator`, two packages, `enabled:
false` in `packages/default-addons.json`). It is our own code, inspired by
Zeblote's New Duplicator (blocklandglass.com/addons/addon/562); none of his
code or assets is used. The original v20-era Duplicator was a community
Add-On by Randy and Ephialtes, not stock v20 (Blockland wiki); Plornt later
remade it with saving and loading.

Player side: `/adup` (or `/advdup`) gives a gold wand. Stack mode copies a
brick and everything on it; `/box` switches to box mode, where two clicks
mark opposite corners (a clicked brick's whole box, or the plate cell where
the click met the ground) and everything wholly inside that the player may
build on is copied, with the box outlined in gold while the wand is in hand.
`/mirror` flips the copy left to right as the player faces, `/mirx` and
`/mirz` across the world's axes. `/cut` removes the originals (full trust,
all or none); Ctrl+Z puts them back exactly, events, lights and owner
included. `/fillcolor` paints the originals in the spray colour, one undo.
`/duphelp` lists it. Copies hold at most 5000 bricks (classic: 2000).

Engine seams, all generic (docs/modding/README.md): `copy_box`,
`mirror_copy` (`build`); `cut_copy`, `paint_copy` (`world.edit`);
`show_box`, `hide_box` (`effects`); query `brick_box`; players' `paint`.
Mirror images come from the bricks' own shapes (`crate::mirror`: drawn quads
and collision reflected and compared under each quarter turn, itself first,
then same-size bricks), so Add-On bricks mirror too; a brick with no twin
keeps its shape and still covers the same cells. The mirror is part of the
placement pose (`PlaceBlueprint::mirrored`, like the turn), so the ghost and
the planted copy agree without a round trip. Undoing a cut restores through
`Simulation::restore_group` / `Authority::restore` and renames the
player's later undo steps and copy to the new brick ids.

Timing (release, synthetic plates, this cloud box): placing a copy costs
about 2.2 µs a brick (4096: 9 ms, 8192: 18 ms), so the 5000 limit keeps one
placement near 11 ms. Tests: `crates/sim/tests/advanced_duplicator.rs` (6),
`mirror::tests` (3), client `building` copy test (mirror and outline),
defaults list, command fuzz. Protocol change: `PlaceBlueprint` +1 field,
`Notice::MirrorCopy`, `Notice::SelectionBox` (numbered 67 here; the Gate
assigns the number).

Not done: saving and loading selections between sessions (needs a place to
keep them per player, host or client side), filling a box with new bricks,
moving box corners with the brick keys.
## 2026-09-30 Live map lights: bulbs break dark, Add-Ons switch, dim and recolour (branch `claude/project-thread-evqu3n`)
Max: "I do want lights going out when a bulb breaks" and Add-Ons that
change a map light's colour and brightness; he chose live lights now, with
a fully dynamic Lighting option in a later version. At rest the map looks
exactly as before (v20's baked look).

Renderer: each recovered map light carries a tint (its uniform's channel
word now holds the tint in `yzw`). `decomposed_lightmap` already split the
lightmap into each light's share plus the residual; with any tint set
(`count.y`), each light's share is scaled by its tint, so a light switched
off leaves the baked map and lamp-lit objects alike, and a recoloured one
recolours only its own share. Untinted frames take the old path (no cost).
`SceneRenderer::set_map_light_tints` uploads only when a tint changes.

Script op `set_map_lights([x, y, z], radius, #{ on, color, brightness })`
under the new `lighting` capability stores a sphere rule on the session (at
most 256, radius up to 2000, tint up to 4; same point and radius replaces;
`#{}` resets). Rules replicate whole in `Checkpoint.map_lights` and
`Delta.map_lights` (protocol 67, the Gate renumbers) and each client maps
them onto its own recovered lights, so the host needs no lighting data.
A broken `lightBulbA` or `fluorescentLight` (already replicated as broken
shapes) dims its lights on the client, whatever the rules say. A first
fixed 8-unit reach missed real lights (Gate, `lighting_probe` on f27e822):
the Bedroom bulb's main light (0.53, reach 140) was fitted 19.9 units from
the bulb, its bright light 21 at 11.8, and the Kitchen tubes' lights at
8.8 to 15.9, with light 4 between two tubes (14.5 and 15.9). Rule now: a
recovered light belongs to every light shape within 1.5 times the nearest
shape's distance, up to 24 units, and its brightness is the share of those
still whole (light 4 halves when one tube breaks, goes dark with both).
The Bedroom's broad fill (light 2, 0.06, reach 430, 6.6 from the bulb) is
the bulb's own fill and goes dark with it. Lights fitted farther than 24
from any fixture (window and sun) are never owned. `lighting_probe` prints
each light shape with the recovered lights within 32 units.

Tests: `bri-render --test unified_lighting
switched_off_and_recoloured_map_lights_leave_the_map_and_objects` (Best and
Low), `bri-sim --test script_api
scripts_switch_dim_and_recolour_map_lights_for_everyone`, `bri-net --test
replication map_light_rules_replicate_whole_and_are_checked`, client
`app::tests::a_broken_bulb_switches_off_its_lights_and_rules_tint_the_rest`.
Later: a "Dynamic" Lighting option (fully live map lights and shadows).
## 2026-09-30 Dynamic lighting option (branch `claude/project-thread-evqu3n`, for v0.1.10)
Max chose live lights for v0.1.9 and a fully dynamic option for a later
version; pharzedia asked for a switch away from the baked look that also
recreates the Bedroom lights with no visibility channel. Options > Graphics
"Lighting:" gains "Dynamic" (`$pref::Video::Lighting` 3). The default stays
Unified+Shine (v20's baked look for map surfaces).

In Dynamic the map's own interior surfaces are lit live, not from their
lightmaps: `dynamic_lightmap` in scene.wgsl adds, per pixel, every recovered
light (no cosine, as the map compiler lit; tints apply) as its light cube or
shadow slot lets it reach the surface, and the sun (N.L) through the map
layer and live casters, to the light no recovered light explains. That
leftover is baked per lightmap texel (`map_lighting::DynamicSheet`: the
leak-cleaned decomposition less every light with exact ray visibility) into
material slot 10, which the scene loader reserves for decomposed lightmaps.
Each map light keeps a cube of the map's surfaces (`ShadowSettings::
light_cubes`, drawn once, 24 faces a frame, 256 texels at Best, 128 below;
3 extra shadow layers, 48 MB at Best, 12 MB at Low), so all 24 lights,
including those without a visibility channel (7 on Bedroom), reach exactly
the surfaces they see, at any distance. Lamps with a slot still add brick
and player shadows. Objects shade every light the same way and add
`MapLighting::residual_all` (the residual without any light); it bakes
after the rest (`Bake::bake_staged`), so the other modes never wait for it,
and Dynamic draws as Unified+Shine until it and the Dynamic lightmaps are
in. Shadows off (Minimum): Dynamic draws as Unified+Shine. Bake format 4:
stored bakes from earlier builds bake again once.

Default mode: the shader paths now read a light's reach through
`light_seen` (cubes first, none outside Dynamic), the same values as before;
the map lights uniform grows to 10 KB.

Tests: `bri-render --test unified_lighting dynamic_lighting_lights_map_surfaces_live_from_every_light`
(a light with no channel, the volume hiding it everywhere: lit in front of a
map wall, dark behind it, dark under a slab with a slot, only the leftover
when switched off; fails with cubes disabled), `dynamic_lighting_takes_the_map_floors_sun_from_the_map_layer`,
`bri-render --test map_lighting dynamic_lightmaps_keep_only_the_light_no_recovered_light_explains`,
shadow layout, options and graphics tests. For the Gate: `lighting_probe`
with `BRI_DYNAMIC=1` renders `{view}-dynamic.png` with GPU times, to compare
with a run without it on Bedroom and Kitchen (look and cost), and the 1M
build in the default mode.
## 2026-09-30 Point lights light objects per vertex, as v20 (Kitchen Dark too bright)
Max: a save with street lamps on Kitchen - Dark looked far too bright next
to v20 (grey walls and a dark grey road came out near white). From his
screenshots the map surfaces match v20; the bricks got about 4x v20's
light (v20 walls and road about 0.27, ours clipping at 1).
`lighting_probe` (new `BRI_MAP`, `BRI_BRICK_LIGHTS`, `BRI_TERMS`) on the PC,
Town on KitchenDark: ambient and sun 0; map lights through their channels
p50 0.22 (tops) / 0.26 (sides), residual 0.04, classic volume 0.34, in line
with v20's level, while the lamp bricks' lights (colour x Brightness 5,
radius 10) reach 3.2 near a lamp. The same holds for the player light
(AutoLight on dark maps, also Brightness 5).
Cause: point lights shaded per pixel with a smooth (1 - d/r)^2 falloff.
v20's brick batcher lights with fixed-function GL (0x531860,
docs/audits/bricks.md): per vertex, N.L / (1 + 0.1 d^2), so a lamp lights
the small bricks it is near and a baseplate only at the corners it reaches;
the road beside v20's lamps shows no pool at all.
Fix: vertex-lit materials (bricks, players, items, vehicles) take point
lights in the vertex shader with GL's attenuation from each light whose
radius reaches the vertex, interpolated across the face. Map surfaces and
terrain keep the per-pixel light. Also cheaper: the per-pixel light loop
for objects is gone.
Test: `bri-render --test persistent_scene
point_lights_light_objects_per_vertex_with_v20_attenuation` (exact GL
attenuation at the corners; a light over the middle of a large face whose
corners it does not reach leaves it dark; fails on the per-pixel shading).

## 2026-09-30 Dynamic lighting rework after the Gate's renders (for v0.1.9)
Max moved Dynamic into v0.1.9. The Gate rendered 0e88aa5 at Best on
Bedroom and Kitchen (spawn and overview). Dynamic cost 0.76-2.27 ms against
0.78-1.27 ms for Unified+Shine, and it had artifacts: light leaking along
Kitchen edges, a web of streaks on the ceiling and around the arched window,
washed-out cabinets, a speckled outline on the Bedroom sun patch, dotted
bright seams where ceiling meets wall, and acne on the desk lamp base.
Root causes:
- Lightmap texels just outside a surface, which bilinear filtering blends
  into its edge, kept the whole decomposition (no light taken out), so the
  lights were added on top of light that was already there. That made the
  seams and the webs.
- The 256-texel light cubes, with normal offsets big enough to avoid acne,
  let light past thin geometry (the streaks and ghosted cabinets). With
  smaller offsets they gave acne.
- The sun on map surfaces came from the map layer, whose texels showed in
  the sun patch's outline.
- The cost was 24 lights, each taking four cube taps per pixel.

The rework takes a light's reach on map surfaces from the bake's exact rays,
per lightmap texel, as a UE stationary light's shadow map does:
- `DynamicSheet` holds the leftover light (RGB) and the baked sun share (A),
  plus, per light that reaches the sheet (up to 24), the share of it each
  texel receives. It stores four lights to an RGBA image in material slots
  1..=6, the diffuse layers only terrain uses.
- A texel's share is the part of its decomposed light that the visible
  lights explain, capped at 1, so at rest the sheet gives back the
  decomposition.
- Texels within 1.5 texels outside a surface (`RIM`) are lit from the
  nearest point of their own surface, nudged 0.05 units inward. Bilinear
  filtering then blends matching values, so no seams.
- The sun on map surfaces is the baked share, capped by live casters' sun
  shadows. Its edges are the lightmap's own.
- Light cubes are now drawn only for lights without a visibility channel
  (7 on Bedroom), and only objects read them.
- The client equips a map's materials with the sheets only when Dynamic is
  chosen (`DynamicSheet::equip`), then uploads the scene again, so the
  other modes carry no extra images. Bake format 5.
- llvmpipe's JIT crashed on a branch that depended on a texel's share
  around the lamp shadow taps. The loop now branches only on the light.

Tests (all pass here):
- `bri-render --test unified_lighting`:
  - `dynamic_lighting_lights_map_surfaces_live_from_every_light`: per-texel
    reach on the floor; a block in front of a wall is lit and a block behind
    it is not, through the cube. It fails with cubes off (120 behind).
  - `dynamic_lighting_takes_the_map_floors_sun_from_its_baked_share`.
- `bri-render --test map_lighting dynamic_sheets_keep_only_the_light_no_recovered_light_explains`:
  leftover light, shares and rim texels (fails with `RIM` 0), equip, and a
  stored round trip.
- The full `bri-render` and `bri-ui` suites, client lib tests, and clippy
  `-D warnings` on render, ui and client lib/bins.
- Next: the Gate re-renders the same views with `lighting_probe`
  (`BRI_DYNAMIC=1` and without) with GPU times, and the 1M build in the
  default mode.
## 2026-09-30 Ragdoll keeps hats, capes and packs on (branch `claude/blockhead-ragdoll-ee3dyw`)

Max's v0.1.8 playtest: the Ragdoll "working pretty good", but capes and
helmets separated from it. Nodes the ragdoll does not place kept their
animated place relative to their parent, and accessories the rig hangs
beside the body's parts (not under them) have no placed parent, so they
stayed where the corpse's death animation left them. Now
`avatar::follow_anchors` gives each node with no placed node above it the
placed node its drawn geometry is nearest in the animated pose, and it
rides rigidly with that one (a hat with the head, a cape or pack with the
torso). Generic for any `avatar.pose` Add-On; no change at the moment the
pose takes over. Tests: `avatar::tests::accessories_beside_the_posed_parts_ride_with_the_nearest_one`;
on content, `ragdoll_keeps_accessories_on` (every hat, accent, pack and
second pack: every drawn vertex within 0.6 of a ragdoll box after the fall).
Also: the Ragdoll tests use `Budgets::untimed` (fuel limits only), so a
loaded gate machine cannot stop the code mid-test.


## 2026-09-30 Ragdoll limbs stay on their joints (branch `claude/blockhead-ragdoll-ee3dyw`)

Max's same playtest: "a few other deformities too". Measured on the
headless ragdoll with a rocket-sized throw (corpse velocity 8, 25, 15): the
limbs came apart at the joints by up to 0.19 units on landing, so arms and
legs hung off the torso. Two causes. Impulse joints are solved iteratively
and give under a hard hit; and Rapier's swept CCD moves a fast body back
along its sweep one body at a time, which alone pulled a limb 0.225 away
from the rest. Now `AddOnPhysics` joins bodies with multibody (reduced
coordinate) joints, which cannot stretch; a joint that would close a loop
falls back to an impulse joint. Because a multibody owns its links'
velocities, pushes, shots and holds are applied as forces over one step
(holds carry the whole chain's mass). The Add-On world uses soft CCD
(`soft_ccd_prediction` of one step at `MAX_SPEED`, swept CCD off), which
adds contacts ahead of a fast body instead of moving it back. Tests:
`the_ragdoll_stays_joined_through_a_blast` (every limb thrown, joints under
0.05 apart, now 0.000), `a_body_at_top_speed_stops_on_a_thin_brick` (a body
at 200 u/s stops on one brick). Client-only; no protocol change.

Accessory check, revised after the Gate's run of `ragdoll_keeps_accessories_on`:
two outfits failed the old rule (every vertex within 0.6 of some box) by 0.01,
both at the same vertex. The rule was the problem, not the placement: a
vertex that sits far from every box standing up (a pointed helmet's tip
above the head box) stays that far when it rides correctly. The test now
checks what the ragdoll promises: every vertex rides with some box, no
further from it lying than it was when the ragdoll was made (0.1 for one
frame of motion). It also cycles every choice of every slot, not only
hats and packs, so skirts (whose hip and trims replace the pants and
shoes) and other parts are covered.

## 2026-09-30 Brick Damage minigames break saves loaded with ownership (branch `claude/minigame-brick-damage-ownership-h6wi1p`)

Max: a save loaded with ownership could be painted and hammered, but his
Brick Damage minigame's weapons left it alone; loaded without ownership, it
broke. Cause: on internet hosts `blow_up_bricks` asks `miniGameCanDamage`
with the brick's owner number. A save loaded with ownership keeps its
builders' numbers (v20 BL_IDs with no identity behind them, or the player
under an earlier number), no connected player has that number, so the
bricks were in no minigame. The host could still hammer them because the
host is an administrator. Without ownership the loader owns every brick.

The brick group now resolves like trust does. `Session::brick_group_player`
finds the connected player a group answers to: its own number, or the same
principal under another number. `Session::brick_group_owner_for` then counts
a group nobody connected answers to as the minigame owner's bricks when that
owner has Full trust over it (administrator, trust given, public domain),
since they may paint and hammer it anyway. v20 left such bricks outside
every minigame; this is a deliberate deviation. A non-admin without trust
still cannot break an absent builder's bricks, and connected players' bricks
are unchanged. The same resolution picks the MiniGame event target for
brick inputs, so a loaded arena's `MiniGame` events reach the minigame.
Outside minigames, internet shooters also break bricks of their own
identity under an earlier number.

Test: `brick_damage::a_brick_damage_minigame_breaks_a_save_its_owner_may_hammer`
(content-free; fails without the fix with 0 of 4 bricks broken). No wire
protocol change.

## 2026-09-30 Mirror debris keeps reflecting until it fades (branch `claude/project-thread-qy54iv`)

Max: a mirror brick destroyed with a hammer should keep reflecting while its
debris flies off and disappears, like every other brick's, not shatter.

Before, `MirrorIndex::mirrors` dropped a dead brick's mirrors the moment its
kill cue arrived. Now `mirrors::debris` poses each debris piece's mirror quads
(the definition's `reflection`, in the brick's own frame, like the debris
model) with that piece's transform every frame and multiplies the mirror's
strength by its fade, so the reflection fades with the brick. Both deaths
carry it: a hammer kill's v20 hop and fall-through, and a blast's tumbling
Rapier body (and its early-eviction ghost). The reflection renderer already
plans every frame from scratch, so a moving mirror needs nothing new there;
debris pieces compete for the Reflections setting's live planes by screen
area like placed mirrors, and the rest show silver. At most the 64 nearest
debris bricks carry mirrors (`MAX_DEBRIS_MIRRORS`); with no mirror brick's
debris alive the cost is one definition lookup per debris piece. The
first-person body is drawn into reflections while mirror debris exists.
Client-only; no wire protocol change.

Tests: `mirrors::tests` (debris mirror rides its body and fades out; chain
kill keeps the nearest 64), `brick_debris` tests unchanged and passing.
Not verified here: the look in game (Max's feel check).

## 2026-09-30 Player names: every character the fonts draw (branch `claude/player-name-characters-4qxbyi`)

A player told Max names "wouldn't let me do special chars". The name rules
already matched v20: the name boxes take every Windows-1252 character (the
v20 font caches hold codes 32-255, so accents and symbols such as `é ñ © ™ €
! @ # $ %` all draw), the host drops `<...>` tags and control characters
(`StripMLControlChars`) and cuts to 23 characters (clan tags 4). Emoji and
non-Latin scripts (★, Cyrillic, CJK) are refused because no v20 font has
glyphs for them; v20 had the same limit. Widening that needs a Unicode
fallback font across all UI and nametag text, which is a separate feature.

Three real problems were fixed:
- A name of 17 or more three-byte symbols (`™`, `…`, `—`, `€`) failed the
  whole join with "Invalid owner name": `OwnerRecord::validate` capped names
  at 48 bytes. It now counts characters (`MAX_OWNER_NAME` 48).
- `~` could not be typed in any text box: Shift+` fell back to the bare-key
  `toggleConsole` global bind. While a text box has focus, Shift/AltGr chords
  now match the global map exactly, so they type; bare ` still toggles the
  console.
- The host keeps only what the fonts draw (a modded client could otherwise
  send characters that show as `?`), drops the invisible soft hyphen and
  turns a no-break space into a space, so no name can pass for another with
  invisible characters. Duplicate-name checks ignore case beyond ASCII
  (`ÉMILE` and `émile`).

Tests: `bri-sim --test session names_keep_every_character_the_fonts_draw`,
`bri-ui --test console shift_tilde_types_a_tilde_while_a_text_box_has_focus`;
clippy clean on bri-sim, bri-ui, bri-world. No wire protocol change.

## 2026-09-30 Reversed depth: close surfaces stop fighting far away (branch `claude/distant-model-lod-vkgoxn`, for v0.1.10)

Max saw distant mirrors, vehicles and players "a bit wonky": jagged red
streaks from a mirror wall's frames across its glass, and split-looking
parts on a far jeep. Cause: depth precision. The world drew with forward
0..1 depth into `Depth32Float` with the near plane at 0.05 and far at 4000,
so two surfaces `d` units out resolved only when about `1.2e-6 * d^2`
apart. A mirror sits 1 mm over its brick (`Reflection::quads`), so it lost
to the brick from about 30 units away; a model's layered faces did the same
a little further out. (Every model already draws its highest detail level:
no LOD switching is involved.)

Fix, as current engines do it: reversed depth. `bri_render::scene`
now owns the convention (`perspective`, `DEPTH_CLEAR` 0, `DEPTH_NEARER`
GreaterEqual, `NEAR_DEPTH`/`FAR_DEPTH`), and every pass drawing into the
world's depth uses it: scene, terrain, sky (pinned to depth 0), mirror
surfaces and passes, lines, foliage, particles, weather and Add-On client
layers (view space sits at depth 0.999..1, screen space at 1). The mirror
passes' oblique near plane is rederived for reversed depth; shadow cascades
unproject the near/far ends from the constants. Shadow maps keep their own
forward depth. Float depth plus the reversed divide keeps surfaces apart
to about a ten-millionth of their distance: 1 mm holds past 1000 units. No
extra GPU work.

Tests: `bri-render --test depth_precision` (both fail on the old
projection: a 1 mm floor over another loses at 80-400 units), updated
`persistent_scene`, reflection, weather, foliage and fx depth tests;
clippy clean (except the known Linux-only `sampler.rs` import).
Not verified here: stock content renders (jeep, mirrors) on the PC.
## 2026-09-30 The host decides which Add-On code runs (branch `claude/host-controlled-addons-ymzypd`)

A joiner did not see Max's ragdoll: the Ragdoll was a `client` Add-On (only
client code), so each player's own list decided, and even a server's copy
that downloaded never ran (a join that brought no bricks, weapons or
vehicles kept the joiner's own Add-On code). Max: it should be server
controlled.

The rule, generic in `bri_package::library::side_for_package`: client code
makes an Add-On `shared` (`CodeOwner::Host`), so a server running it offers
it, every joiner downloads the host's copy and runs it for that game, and a
joiner's own copy sits out a game whose host does not run it. An author
marks code for one player's own screen with `"personal": true` in the
`client` section (`CodeOwner::Player`), which keeps it `client`: it runs
wherever that player plays and a host's personal code never runs on a
joiner's screen. Client code on a `server` Add-On is refused as mixed
sides. A list's `side` follows the manifest when loaded
(`follow_manifest_sides`), so lists written by earlier games (Max's own,
with the Ragdoll listed `client`) follow too. The packagers use the same rule.

The game now loads the Add-On code of the list it joined with
(`Attempt::joined`, `ClientCode::loaded_from`). Code the player installed on
this PC runs on a server without a trust prompt, whether they turned it on
or not (byte-identical code hash); anything else a server sent still asks.
Ragdoll, Gravity Gun Effects and Steel Ball Shine are now host-controlled;
the Gravity Gun, its tool and Portals already were (`server`/`shared`).
No wire protocol change: only the package list and downloads travel.

Tests: `bri-package library::tests::the_host_decides_on_client_code_unless_it_is_personal`,
`a_listed_side_follows_the_add_ons_manifest`; `bri-client --lib client_code`
(installed code runs unasked, sent code asks, a host's personal Add-On never
runs); `add_on_join` step 4 (needs two content roots, PC only).
above the head box) stays that far when it rides correctly. The first
replacement (no further from some box lying than standing) was hollow: the
Gate saw 0.00 for every outfit, because a vertex only had to get no further
from any one box, and nearly always some box came closer. The check now
takes each vertex's place in each box's own frame, standing when the
ragdoll is made and lying after 4 s, from the bodies the last pose read;
the vertex must keep its place (0.01) in some box's frame, as a part riding
that box rigidly does. It requires the boxes to have fallen at least 0.5,
cycles every choice of every slot (skirts included), and proves itself: it
reruns every outfit with `follow_anchors` turned off (a test-only switch)
and fails unless some vertex then moves at least 0.5.

## 2026-09-30 Ragdolls and debris stay on map floors (branch `claude/blockhead-ragdoll-ee3dyw`)

Max, v0.1.8: "my ragdoll sometimes fall through the bedroom floor". Map
interiors and static models are triangle meshes built with
`FIX_INTERNAL_EDGES`, which drops any contact that comes from a triangle's
back. A floor is one layer of triangles, so where its triangles face down
(authored the other way round) or a limb reaches it from behind, bodies on
this client's own physics fall through. Reproduced headless: the Ragdoll on
a down-facing floor fell to y -11 lying still and -25 blasted down; on an
up-facing floor it stayed on top. Now `Building::new` makes every map
triangle mesh `FIX_INTERNAL_EDGES_TWO_SIDED` (the client's map world is
used for rays and these local bodies only; server and prediction collision
are unchanged), and `Surroundings` reloads the map's colliders whenever
`Building::map_generation` changes (another map, or a map shape smashed)
instead of loading them once per Add-On world. Whether the Bedroom's floor
triangles face down is inferred from the mechanism, not measured on its
content. Tests: `a_ragdoll_lies_on_a_map_floor_whichever_way_it_faces`,
`bodies_stand_on_the_map_they_are_in_now` (fails with the old load-once).
## 2026-09-30 Player names: other scripts, symbols and emoji (branch `claude/player-name-characters-4qxbyi`)

A player told Max names "wouldn't let me do special chars". The rules matched
v20: the name boxes took the Windows-1252 characters the v20 font caches hold
(codes 32-255: accents and `! @ # $ % © ™ €`), the host dropped `<...>` tags
and control characters (`StripMLControlChars`) and cut to 23 characters (clan
tags 4). Emoji and other scripts were refused because no cache had glyphs.
Max: "we should probably allow it".

- `bri_ui::fallback`: a glyph a cache lacks is rasterised (ab_glyph) from the
  first system font that has it, the cache's own face (Arial) and each
  platform's broad-coverage fonts first, then every font in the system font
  folders; scaled so its ascent matches the cache's baseline. This is what
  Torque's `GFont` did for glyphs missing from a cache. Outline glyphs are
  coverage, tinted and outlined like cache glyphs; colour bitmap emoji keep
  their colours and get no outline. Fonts are memory-mapped once per
  process, only when such a character is first drawn. A character no font
  has still draws the cache's `?`. Every text path (names, nametags, chat,
  lists) goes through `text::Font`, so all of them draw these.
- `bri_console::names::name_char` is the one list of what names and clan tags
  may hold, used by the name boxes (typing) and the host (cleaning): v20's
  set plus Greek, Cyrillic, Armenian, Georgian, CJK, kana, Hangul, symbols,
  arrows, shapes and single-character emoji. Left out, because one character
  at a time cannot draw them right or they hide things: joined or reordered
  scripts (Arabic, Hebrew, Indic, Thai), combining marks (Zalgo text),
  zero-width, bidi and other invisible characters, blank fillers, skin tones
  and flags. No-break and ideographic spaces become plain spaces.
- Lookalike names: `names::skeleton` folds case, fullwidth letters, and
  Cyrillic/Greek letters that look Latin (`Мах`, `Μax`), and `I l 1 |`, `0 o`,
  as UTS #39 skeletons do for these scripts. A joining or renaming player
  whose name reads as a connected player's gets a number ("Мах 2").
- Fixed on the way: a name of 17+ three-byte symbols (`™`, `…`) failed the
  whole join with "Invalid owner name" (`OwnerRecord` capped at 48 bytes; now
  48 characters), and `~` could not be typed in any text box (Shift+` fell back
  to the bare-key `toggleConsole` global bind; while a text box has focus
  Shift/AltGr chords now match the global map exactly, bare ` still toggles).

Tests: `bri-console names`, `bri-sim --test session
names_keep_other_scripts_symbols_and_emoji`, `bri-ui --lib
characters_the_cache_lacks_come_from_the_fallback_fonts`,
`name_and_clan_boxes_take_other_scripts_symbols_and_emoji`,
`system_fonts_draw_what_they_cover`, `bri-ui --test console
shift_tilde_types_a_tilde_while_a_text_box_has_focus`. A render of
"Max Жора Ωmega 小明 たろう 민수 ★♥☺→ 😀🎮" from the Linux container's fonts at
the v20 size-14 baseline drew every character. No wire protocol change: names
were already UTF-8 strings.

## 2026-09-30 Ragdolls slide down ramps and stay down (branch `claude/blockhead-ragdoll-ee3dyw`)

Max, v0.1.8: "if my ragdoll slides down some ramps it goes down and then
magically climbs back up". The Ragdoll pulled any ragdoll more than 2.5
units from its corpse back towards it, and the corpse stays where the
player died, so a ragdoll that slid further down a steep roof was dragged
back up (reproduced: on a 50 degree roof it slid 4.2 down and was hauled
back up 1.1 and held there). The pull is gone; the ragdoll gives up only if
it falls 60 below its corpse (out of the world). So the dead still see
their ragdoll, the orbit (death and spy) camera now follows a body that
Add-On code poses: `AvatarMesh::drawn_offset` (the drawn nodes' middle less
the animated ones') moves the orbit focus, which is unchanged the moment
the pose takes over.

Found on the way: joining bodies into a multibody (c9201f7be) started it
still, so a ragdoll lost its corpse's motion and pop. `AddOnPhysics::join`
now carries the root body's velocity into the multibody's free root.

Tests: `a_ragdoll_slides_down_a_ramp_and_stays_down` (fails with the old
module: held at x 3.05 on the roof), `jointed_bodies_keep_the_motion_they_were_made_with`,
and the floor tests now use a floor big enough for a thrown ragdoll to land
on (they had relied on the pull). Not verified here: the camera follow in
the game (Max's feel check).
## 2026-09-30: held items stay put in a rolling or looping vehicle

Max's report: in the Stunt Plane's first-person view, looping or rolling
threw the held paint can, and every other tool and weapon, to the top of the
screen. The first-person image was placed in an eye frame built from the view
yaw and pitch only, while the camera also rolls with the seat
(`controls::roll`). Torque draws a first-person image in the eye's frame and
the eye is the camera, so the local player's first-person image now uses the
rendered camera's frame (`controls::view_frame`: position, yaw, pitch and
roll). This covers every vehicle, seat and held image, and also a
player-type mount whose camera heading comes from the mount. Other players'
images and third person are unchanged.

Test: `bri-client --lib app::tests::a_first_person_image_stays_on_screen_through_a_loop`
(an eye offset keeps its screen position for any yaw, pitch and roll against
the renderer's `rolled_view_basis`). Clippy clean on the client lib. No wire
protocol change.
## 2026-09-30: Blockhead Bot is an Add-On; bots find their way and fight like players

Max (thread "Blockhead Bots"): make the Blockhead Bot an Add-On, and give
bots a smarter brain and path finding. The v20 decompiled scripts
(`fxDTSBrick::spawnVehicle` allGameScripts 16844-16964; the spawn list,
client 15786-15807) show a Vehicle Spawn brick makes an `AIPlayer` only for
rideable player types and that nothing in the stock scripts moves or aims
one, so v20's spawn-brick bots stand still. Our walking, fighting bot is
beyond v20 and now ships as the optional **Blockhead Bot** Add-On
(`packages/blockhead_bot`, installed but off).

- New package kind `bots` (`assets/bots.json`, shared: players load it for
  the wrench list). `bri_sim::bot_kind` holds the schema; the base game
  provides no bot kinds. Host: `content_identity::bot_kinds` →
  `Session::set_bot_kinds`; client wrench list from the same providers. The
  kind id stays `bot.blockhead`, so saves and the Gravity Gun's bot views
  keep working.
- Path finding (`bri_sim::nav`): a half-stud walk grid sampled on demand
  from fixed collision (chunk colliders, map, terrain), with steps, jumps,
  drops, slopes and ceilings; paths keep off walls through the middle of
  doors. Samples are remembered; when bricks change, the rebuilt chunks'
  boxes (`Simulation::take_collision_changes`) forget only the samples they
  touch. A* resumes across ticks: all bots together take at most 96 new
  samples and 384 expansions a tick. With no bots nothing is tracked.
- Brain (`session/bots.rs`): aim turns at the kind's rate with a reaction
  delay and error that narrows while it keeps sight; fighting distance from
  the held weapon (melee closes in, explosive keeps clear of its blast,
  arcing shots lead and aim for the drop); turns on whoever hurt it and
  searches where it last saw an enemy; leashed to its brick; fights other
  builders' bots, never its own builder's.
- Measured (release, 16 bots in a 800-column maze on the synthetic map,
  30 s): whole session tick 0.099 ms average with bots vs 0.008 ms without,
  worst tick 0.78 ms. No wire protocol change: bots are players.

Tests: `bri-sim nav::tests` (7: straight line, round a wall through the
door's middle, step vs jump, partial path at an unjumpable wall, low
ceiling and local invalidation, per-tick budget, stairs and determinism),
`bot_kind::tests`, and `bri-chaos --test bot_brain` (walks round a
see-through wall the old brain got stuck on; reacts then hits, on the same
tick every run; one builder's bots don't fight; no Add-On, no bot).
Not verified here: how the bots feel in play (Max's check).

`/clearBots` now also clears player-type mounts (horses, boats, cannons,
turrets) no rider controls, as v20's `ServerCmdClearBots` (G:5089-5133)
deletes every player object no client controls; those mounts are players
in v20. A ridden mount stays, and its spawn brick keeps its setting. Test:
`bot_brain::clear_bots_also_clears_the_mounts_nobody_rides`.

## 2026-09-30 Add-On weapon seams from a modder's second write-up (branch `claude/project-thread-n5mlwe`, protocol 68, for v0.1.10)

A modder sent a second list of engine changes for porting guns. Their patch
was not applied; each claim was checked against main and v20's engine
behaviour, and only the real ones were fixed, each with a deterministic test
using our own fixtures.

- Real, fixed: the next gun mounted empty (runtime kept the last image's
  ammo flag; `mountImage`/`WeaponImage::onMount` mount loaded;
  `crates/weapons/tests/addon_seams.rs` failed with "Empty" before the fix);
  Add-On `AudioProfile`s never played (importer now converts them to pack
  sounds and rewrites state/projectile/explosion references); a nameless
  `ItemData` failed the whole pack (now left out with a note, image kept;
  packs require `ui_name`); a missing kill icon dropped the damage type
  (icon stripped, type kept); emitter fields the engine corrects in
  `ParticleEmitterData::onAdd` (period, variance, theta) are now corrected
  by the converter.
- Already fixed on main: the off-centre scope.
- New seams (capability-gated, bounded): `take_item`, `drop_item` (64 live
  drops per package); hooks `on_pickup`, `on_drop` (its value rides the drop),
  `on_projectile_hit` (256 pending per tick), scoped to the package's own
  content or a dependency's; `tool_only` commands; `commands.cancel`, which
  needs `Command::CancelBrick` on the wire (protocol 68); `mx/my/mz` muzzle
  and `tools` in the player map.
- Importer: an Add-On's own emitters (image states, trails, explosions),
  explosion bursts and lights go into a new `effects` section of its weapons
  pack, which the client merges (base game ids win); owned `DebrisData` is
  kept as definitions.
- `stateEmitterTime` 300 s cap left out (v20 has none).
- Follow-up, same branch (Max 21:48Z: gaps go into v0.1.10): Add-On casings,
  explosion debris and particle textures. `bri_weapons::debris::casings`
  reads an image's `casing` DebrisData and `shellExit*` fields (shared
  field reader with `explosion_debris`). The importer lists casing and
  debris models and the Add-On's own particle textures in its item
  presentation. Client: `WeaponDebris::set_casings` gives an image with
  its own casing model its own motion (clamped to the stock ranges, at
  most 256 kinds) and `model_instances`; `WorldItems::set_loose` draws
  those and Add-On explosion debris that no vehicle model covers (at most
  512); `WeaponEffects::with_textures` adds an Add-On's particle textures
  from the item presentation, at most 64, each fitted within 256 px (the
  effects texture array is as large as its largest layer). The base
  game's `gunShellDebris` keeps the stock shell. Tests:
  `weapon_debris::tests::an_add_on_casing_throws_its_own_model_and_motion`,
  `weapon_effects::an_add_on_particle_draws_its_own_texture`,
  `addon_seams::an_images_casing_reads_its_debris_and_shell_fields`, and
  the importer's synthetic kit.
- Held-image placement vs Torque (the modder's third write-up, found on
  v0.1.8): first-person scopes drifted because c583791 (2026-09-29) made every
  first-person `eyeOffset` image ride the arm's thread-2/3 actions. Torque's
  `getRenderImageTransform` places it at eye × eyeOffset alone. New image
  field `follow_arm` (weapons pack and presentation, client-side, default
  off, no wire change); the base game's content turns it on for its own
  images so the brick, hammer and spray cans keep the jolt Maxwell asked for.
  The eye is the drawn camera already (411257b, v0.1.9), and third person
  stays hand mount × offset/rotation × mountPoint⁻¹. Import Add-On lost an
  Add-On's `eyeRotation` written as axis-angle or `eulerToMatrix`; it now
  reads the pack's parsed value, and axis-angle rotations about any axis
  convert (`bri_weapons::rotation::axis_angle`). A finished one-shot arm
  action is left holding its end pose, as Torque holds a finished thread.
  Test: `items::placement_tests` (scope-style and v20-tool images, several
  offsets, first and third person).
- Tests: `crates/weapons/tests/addon_seams.rs`, `crates/sim/tests/item_hooks.rs`,
  `crates/addon-import/tests/import.rs`
  (`a_gun_add_on_brings_its_sounds_effects_debris_and_odd_items`, fixture
  generated at test time), `crates/client/tests/weapon_effects.rs`,
  `crates/convert/src/effects.rs` onAdd test.
## 2026-09-30: Bedroom with 200k bricks (Max: about 80 fps)

Max stacked random saves in Bedroom (about 200k bricks) at 3440x1440 with
Unified+Shine, Best shadows and Brick Shadows on, and saw about 80 fps.

Measuring: `SceneRenderer::time_passes` stamps GPU time per stretch (sun
shadows, lamp shadows, mirrors, world, effects; `bri_render::timing`), shown
in the expanded F3 overlay and reported per view by `large_build_perf`,
which now also stacks several saves (`BRI_PERF_SAVE="a.bls;b.bls"`), takes
`BRI_PERF_MAP`, and reports entity counts. PC run on c3505ba4 (198,602
bricks placed, 151 light bricks, 488 emitters): inside the build frame 9.7
ms = update 1.2 + CPU recording 5.0 + GPU 3.3 (world 2.9, sun 0.3); spawn
5.9 ms. The 105 s load it reported was the harness waiting 20 s after each
stacked part; the settle waits are no longer counted.

Changes:
- Brick sun shadows are kept per cascade between frames
  (`kept_shadows`): a layer twice the cascade's width on its texel grid,
  copied in each frame with only moving casters drawn on top, redrawn by
  region when bricks change. A cascade is kept only with 250k brick
  triangles in reach (a copy costs about what drawing 150k does), released
  below half that. Spawn sun shadows 0.64 -> 0.21 ms.
- Brick occluders (Brick Shadows off) draw only the chunks under a caster,
  scissored to the casters' footprint.
- Point lights are binned into a world-space grid over their reach
  (`light_grid`), so a pixel adds up only the lights listed for its cell
  instead of all of them (up to 256). View-independent, so mirrors read it
  too. Tests: `a_full_light_budget_lights_each_pixel_with_the_lights_that_reach_it`,
  `kept_brick_shadows_match_drawing_every_brick`,
  `occluders_draw_only_the_chunks_under_a_caster`.

- Rebased on the per-vertex point lights (Kitchen Dark): the vertex path
  reads the grid too.
- Run D (dfb6e65f, same saves): load 5.2 s (the 105 s was the harness);
  overview now shows the city: frame 8.5 ms, GPU 2.6 (world 2.1, sun 0.39),
  record 4.6, 508 chunks, 1.9M triangles, 44k particles drawn. Inside: 8.8
  ms, GPU 2.6, record 4.9. The inside CPU profile: 32% waiting on the GPU
  (the harness waits each frame), render_scene 43%, of which the effects
  snapshot 20%, effects advance 8%, combining effect frames 4%, particle
  upload 4%; wgpu encoder finish 11%, render-pass encoding 9%.
- So past 4096 particles, sampling runs on rayon's worker threads in
  ordered chunks (identical result:
  `a_crowd_sampled_on_threads_matches_one_thread`), and a particle out of
  view even at its largest authored size is skipped before sampling.
  Run E (0f5a8d5f) showed the first version, which spawned up to 8 threads
  per advance and per snapshot, cost more than it saved on Windows: update
  +0.4 ms and spawn-view record +0.5 ms in every view. Advancing is back on
  one thread (splitting it saved 0.1 ms of 1.0 ms locally at 60k
  particles); sampling uses the kept-running rayon pool. Locally (4 cores,
  60k particles): snapshot 2.9 -> 2.2 ms looking at them, 0.76 -> 0.35 ms
  looking away.

Open: offscreen, CPU (6 ms) and GPU (2.6 ms) overlap in the game, which
would be well above 100 fps; Max's 81 fps at 95% GPU is not reproduced by
this benchmark. The expanded F3 overlay now lists GPU time per pass, so
his next report can name the pass.
## 2026-09-30 Portals: no blank screen halfway through, no crash near a pair (branch `claude/portal-bricks-5be9t8`)

Max, v0.1.9: halfway through a portal the first-person screen flashed flat
blue-grey (the portal's idle colour) and third person flickered; placing a
portal near another crashed (wgpu: 'mirror reflection' texture used as
RESOURCE and COLOR_TARGET in one pass).

Causes: (1) with the eye closer to a window than the near plane (0.05) its
quad was clipped away, so `seen` found it off screen and it lost its pass,
while its recess box (drawn so it never clips) still covered the screen in
the fallback colour. (2) A portal's view is clipped at its partner's
plane, not its own, so its own window was not left out of its own pass
and drew with its own picture (`Shows::Echo(i)` in view `1 + i`) whenever
it was drawn there. (3) The chase camera's boom did not go back through
the opening once the body came out of the partner.

Fix: `seen` measures a recessed window by its box and keeps it with the
eye inside the box; a live view's eye keeps 1 cm behind the plane it is
clipped at (`CLIP_CLEARANCE`); `Plan::slots` never lets a view show the
picture it is drawing (it shows the fallback, as surfaces past the passes
do); the recess applies only strictly in front, as an uncarried eye is;
`Building::camera_boom` carries the chase camera back through openings
and the look turns with it. Tests: `reflection::tests::
a_view_never_draws_with_the_picture_it_is_drawing` and
`a_window_the_eye_is_passing_through_stays_live_past_the_near_plane` (both
fail without the fix), `building::tests::
the_chase_camera_boom_goes_back_through_a_portal`. Not verified here (no
GPU): the look while crossing.

## 2026-09-30 Dynamic: a broken bulb leaves the room as baked, without its light (for v0.1.10)
Max (v0.1.9, Dynamic) broke the Bedroom lamp's bulb. Big, blocky shadow
shapes stayed on the ceiling and the upper wall behind the lamp, that wall
stayed a flat lit grey, and a bright strip ran up beside the shade. Cause:
each texel's baked light was split between the lights by the bake's rays
alone. The rays disagree with the map compiler near the lamp. Its shade and
frame hid the lamp from texels the lightmap shows lit, so that light stayed
in the leftover and the wall stayed lit with the bulb gone. The compiler's
shade-frame shadows, where the rays see the lamp, lost their ambient and
the sun's ambient to the lamp, so they turned darker than the room: the
shadows outlived the light. Both steps are one lightmap texel coarse, hence
the blocks.

Fix (`Bake::dynamic_sheets`): the interior's own lightmap decides how much
light arrived, and the rays only say which light it most likely was.
- A texel's authored light above the compiler's ambient
  (`authored_floor`: the 5th percentile of the texels no light reaches by
  the rays, or none with too few) goes first to the lights its rays see,
  up to what it holds.
- The rest goes to the lights in reach the rays say are hidden, when it is
  more than a tenth of their light (fading in up to a quarter). Smaller
  remainders are fit error or untraced lights, and stay in the leftover.
- The ambient and the sun's ambient never go to a light.
- Bake format 6.

At rest nothing changes, since every texel still sums to its baked value.
With a light off, its baked shadows and glow go away with it.

`lighting_probe` gains `BRI_OFF=i,j,...` to switch recovered lights off
(the "Light shape" lines list each bulb's lights).

Test: `bri-render --test map_lighting a_switched_off_light_leaves_only_ambient_where_rays_and_lightmap_disagree`.
It has a slab the compiler never saw hiding a lit floor, a compiler shadow
on a wall with nothing blocking the rays, and a low table the compiler did
see. With the lights off, the leftover stays within 12 levels of the
ambient, with 95% within 6. It fails with the floor at 0 (23 levels dark
in the wall's shadow) and with no hidden-light attribution (56 levels lit
under the slab). Full `bri-render`, client lib and clippy pass. Not
rendered on Bedroom here (no stock content): the Gate's check is Bedroom
with `BRI_DYNAMIC=1` and `BRI_OFF` set to the bulb's lights, at Max's spot
by the lamp.
## 2026-09-30: remote turret aim no longer flickers; turrets keep their aim

Max's report (v0.1.9): watching someone turn a Tank turret, other clients saw
the barrel snap to a wrong direction for a split second. The host sends the
turret's yaw relative to the hull, wrapped to [-pi, pi); observers' vehicle
interpolation (`vehicles::sample`) blended it with a plain lerp, so a barrel
crossing straight behind (3.1 to -3.1) swept round through the front for a
frame. Turret yaw now blends through `motion::lerp_angle`, the same short-way
blend remote players' yaw already used (now one shared helper). No wire change.

Max also asked whether the turret should keep facing where it was left. It
does now, as in v20 (the turret is its own Player object there): leaving the
gunner's seat or switching seats keeps the aim (`world::idle_controls`), and a
new gunner's client turns its look onto the barrel (`vehicles::turret_look`)
while the host holds the aim until inputs carry that new look (the existing
`mount_yaw` staleness check).

Tests: `bri-client --lib vehicles::tests::a_turret_turning_past_the_hulls_back_never_sweeps_round_the_front`
(fails with the old lerp), `...a_gunner_taking_over_looks_along_the_turret`,
`bri-vehicles --test native the_tank_turret_keeps_its_aim_between_gunners` and
`bri-sim --test vehicles a_new_tank_gunner_takes_the_turret_where_it_was_left`
(both need content; run on the gate).
## 2026-09-30: Gravity Gun wheel reels while holding; letting go adds nothing

Max's report on v0.1.9: holding a jeep, the wheel switched tool slots
instead of reeling it, and letting go of a swung jeep looked like the old
blast kicked it.

The wheel goes to a tool's `wheel` command only while the trigger is held
(`controls.held(Fire)`), but on foot the click goes to the building path,
which never told `controls` the trigger was down. So the tool never took
the wheel. `app::note_trigger` now records the trigger for every click
before it is routed (building, a gunner's seat, the spy camera).

Letting go never pushed anything in the engine (`Op::LetGo` only drops the
hold). What read as a blast was the effects' throw burst: a shockwave ring,
sparks and a launch boom. That burst is gone (ring.wgsl and launch.wav
removed). A let-go of anything now plays the drop sound, and the rule's
public `beam` state shrinks to [held kind, held id, beam on, beam length].
`make_showcase_sounds.py` seeds each Add-On separately, so the Steel Ball's
sounds were regenerated.

Tests: `bri-client --lib app::tests::the_trigger_is_noted_whichever_path_takes_the_click`,
`bri-sim --test showcase letting_go_carries_only_the_swing` (a swung crate
never gains speed after letting go), and the sandbox effects tests (a
let-go draws nothing and plays only the drop). Not verified here: the feel
in game (Max). No wire protocol change.

Follow-up (same day): the Gravity Gun had borrowed the Printer's icon, so
in the tool slots it looked like a second Printer. Add-On items may now name
their own icon PNG (`items::own_icon`: relative to the folder holding
`weapons.json`, stock icons first, bad or missing files fall back to the
letter). The Gravity Gun's icon is original art drawn by
`tools/make_showcase_icons.py`, not a render of the Printer model. It shows
the dark shell, green edges, teal veins and glowing muzzle it has in play.
Test: `bri-client --lib items::add_on_icon_tests::an_add_on_item_shows_its_own_icon`.

Effects polish (same day, Max: "anything a little extra... if it makes
sense"): catching now flashes at the grip (a glow that swells and fades
over 0.25 s), the grip glow pulses gently while holding, and letting go
snaps the beam back from the grip into the muzzle over 0.18 s, fading as
it goes. These are looks only, drawn by gravity-gun-fx from state every
client already has: no gameplay change and no network traffic. Tests: the
sandbox effects tests (the flash, then 6 draws once it's over; the
snap-back halfway at 0.09 s and gone by 0.2 s), plus the offscreen render
on llvmpipe. That render's frame list, cut by mistake in the previous commit,
is restored.

Reaching sound (same day, Max: firing with nothing caught was silent). A
searching whirr (`reach.wav`, generated by `make_showcase_sounds.py`) now
plays at the muzzle when the trigger goes down with nothing caught, and
again every half second while the beam keeps reaching. It stops at the
catch, where the grab sound takes over. Test: the sandbox effects test
checks when it is heard.

Bots (same day, Max: "unable to use the gravity gun on a blockhead bot").
Add-On scripts never saw bots. The package snapshot leaves them out of
`players()`, so `object()` found nothing and the gun's grab did nothing.
Bots are now movable objects (`movable_views`, `object` falls back to them):
a `player:` ref with their kind as `definition` and their spawn brick's
owner as `owner`. `players()` is unchanged. `may_move` takes a bot's owner
to be its spawn brick's owner outside minigames, as for the vehicles such a
brick spawns: the owner, anyone they trust to build, and administrators.
Inside a minigame, bots follow the minigame damage rules as before. Test:
`bri-sim --test showcase a_bot_is_grabbed_like_a_player` (a non-admin brick
owner grabs, lifts and lets go of a bot; a stranger may not).

Held players spinning (same day, Max: "I see them spinning really fast 360"
while on the held player's own screen they hung still). A held player rides
a tumble, and a tumbling player watches through the corpse camera, so their
mouse turns nothing. Their client still sends the seat's world heading as
its move yaw, and the host took that for a passenger's turn on the seat
(`mRot.z`). Every turn of the swing was added a second time, so the body
spun in the beam on everyone else's screen. The host now ignores a tumbling
rider's moves as a turn (as v20 does: the camera, not the body, is their
control object). Clients no longer turn a tumble rider's body by their yaw,
so it rolls with its tumble everywhere and matches what the held player
sees. The server's tumble itself was already steady, measured every tick.
Test: `bri-sim --test showcase a_held_player_turns_only_with_their_tumble`
(swinging a held player half way round, whose client sends what it sends
while tumbling; the body stays on its tumble). It fails without the fix
(0.17 rad off within six ticks).

Held player's own view stuttering (same day, Max: dragging a player looked
smooth to him, but "on their screen it seems a bit stuttering like
teleporting"). The client treated any vehicle whose first seat it sat in as
one it drives. It drew that vehicle ahead of the newest pose on a guess and
pulled it back when the next pose disagreed. A tumble (a held player rides
one) is driven by nobody, so every burst of the holder's pull overshot and
snapped back on the held player's screen, while the holder saw the smooth
interpolated poses. Only a seat that steers now makes its vehicle "driven"
(`driven_vehicle`). A tumbling player sees their body drawn from the host's
poses, as everyone else does. No new network traffic. Tests:
`bri-client --lib only_a_steering_seat_drives_its_vehicle` and
`a_dragged_body_drawn_from_the_hosts_poses_never_steps_back` (a body pulled
in bursts, its poses arriving unevenly: drawn from poses it never steps
back; guessed ahead it does, 33 frames in that run).

Gentle set-downs and throws (same day, Max: "they shouldn't always tumble
if i move them gently and carefully somewhere", then "maybe if i toss them
and they fly and hit a wall"). A held player rides a tumble, and letting go
only dropped the hold, so they tumbled on until the tumble settled. Now
letting go gives a living player their body back at once, with the speed
they had, and the tumble's body is removed then and there, not after the
next step (they would land on it and stop dead). One let go slower than 10
u/s was set down and simply stands. One let go faster was thrown: for up to
3 s, or until they land and slow down, a velocity change of 12 u/s or more
in one tick is a hard impact and tumbles them. That covers a wall met head
on or at a glance, or the ground from a height. A normal landing from a
throw (about 6 u/s) just slides them to a stop on their feet. Test:
`bri-sim --test showcase a_thrown_player_tumbles_only_when_they_hit_something_hard`
(set down slowly: stands; thrown in the open: flies more than 15 units and
slides to a stop, no tumble; the same throw into a wall: tumbles).

Deterministic effects tests (same day). The showcase effects tests ran the
Add-Ons under the game's default budgets, whose frame, GPU and physics
limits are wall-clock: the offscreen render once stopped at 127 ms of
graphics time on llvmpipe's first frame (it compiles its shaders then).
`Budgets::untimed()` now lifts the GPU and physics milliseconds too, and
the showcase tests use it. Instructions (fuel) still bound every call, so
they check what the effects draw, the same on any machine. The game keeps
the timed defaults.

Gravity Gun icon restyled (same day, Max: "please be consistent here with
the game"). The first icon was flat art with a glowing outline and halo,
unlike every other item icon, which is a small lit model on a clear
background. `make_showcase_icons.py` now ray marches a small original model
of the gun (rounded boxes: body, emitter, grip) with a key light, fill and
highlight, seen from above and to the side and pointing up and right like
the Hammer and Wrench. It keeps the gun's in-play looks (dark shell, teal
veins, green-lit edges, teal muzzle), with no outline or glow. It is still
original art, not a render of any game model.

Gravity Gun icon from its model (same day, Max: "take the 3d model +
shaders + snap pic -> make transparent background -> use as the icon just
like the other tools"). The gun in play is the Printer's model (v20
content) under the gravity-gun-fx skin, so a picture of it may not be
committed. The icon is now drawn on each player's machine at item load
(`crate::item_icon_render`). An Add-On item may ship `<icon>.render.json`
naming a stock item to pose like (`pose_like`, here the Printer) and a
look. The stock item's model is fitted to its own icon's outline (every
turn at 15 degrees on a coarse grid, the best 8 refined, then the
framing), which recovers the angle and framing the stock icon was drawn
with. The item's model is drawn the same way, on a clear background, with
the model's tint and `alien.wgsl`'s skin ported to the CPU (puffed along
normals as in play, so the green-tinted model shows at the hard edges).
The shipped PNG stays as the fallback if anything is missing. Tests:
`item_icon_render` (a pose recovered from an icon to 0.9 overlap; a model
that is not the icon does not fit; the skin is dark with teal veins on a
clear background, the same every time; the request is checked), and
`add_on_icon_tests::the_gravity_gun_icon_is_drawn_from_its_model_like_the_printers`
(needs content: the icon is the render, the Printer icon's size, and it
is written to
`target/gravity-gun-icon.png` for a look).

Gravity Gun icon framing and light (same day). The Gate's first render ran
off the top, right and bottom edges and was too dark to read. Cause: the
render reused the Printer's whole fitted pose, scale and centre included,
so a model of another shape overflowed the frame. Now only the angle is
taken from the fit. The model's own projected bounds (with the skin's
puff) are fitted, centred, into the box the Printer's drawing fills, inset
to keep at least 6% of the icon clear on every side. The shell is lit under
a brighter icon light (3x, as stock icons are shot brighter than play) so its
faces read apart. The model's hard edges (welded, faces over 35 degrees
apart) are drawn a pixel wide in the model's green, which is what the
puffed skin shows along them in play but is thinner than a pixel at icon
size. The veins are drawn at least about a pixel wide. New test
`the_icon_keeps_a_clear_margin_on_every_side` (a stock icon drawn edge to
edge, three angles: at least 5 clear rows and columns on every side at 96,
and the drawing still 80 px across one way). The content test now checks
the clear border and that the drawing spans the Printer's width or height,
and writes `target/gravity-gun-icon-vs-printer.png` (dark and light slots;
target/ is never committed).

Gravity Gun icon kept on disk, drawn off the load path, seen side on
(v0.1.11 follow-up). The Gate measured the whole Gravity Gun item load at
286 ms in release (1.65 s in debug), with the icon drawn inside it. Now
`ItemAssets::load_with` only prepares the request (`item_icon_render::Request`:
the item's model, the stock model and icon, the spec).
`ItemAssets::draw_icons(cache)` shows an icon kept under
`<state>/item-icons/<sha256>.png` at once. Otherwise it draws the icon on a
thread named "item icon" and keeps it, written through a partial file. The
sha256 covers a drawing version (`DRAWING`), both meshes and their axes, the
stock icon's pixels and the spec. Until then the HUD slot shows the PNG or
letter, and `ItemUi::register_icons` swaps the drawn icon in and uploads
again. A failed draw is logged to the console.

Angle: Max, in game on v0.1.10, said the icon was "great just seems to be
wrong perspective angle". The outline fit had searched every turn, and the
Printer icon's outline also fitted a tumbled one (seen from above and
behind, nose rolled down). The fit now tries only side profiles
(`item_icon_render::Profile`): the item's own forward across the picture
(either way) and its up up the picture, tipped up to 60 degrees in the
picture and turned up to 46 degrees towards or away. The item is drawn with
the same profile on its own axes (`Axes`: +Y forward and +Z up as item
models are held, or mountPoint to muzzlePoint made level). Max liked the
look, so the relighting tried for this was dropped. Tests:
`a_drawn_icon_is_kept_for_the_same_request`, `item_ui::a_drawn_icon_replaces_its_stand_in`,
`a_models_pose_is_recovered_from_its_icon` (the profile and axis directions
recovered), and `an_item_drawn_like_a_stock_one_points_the_same_way` (a
model built along other axes gets the stock item's screen directions, up up
and nose across). The content test now also checks that the gun's forward
and up point the same ways on screen as the Printer's, and prints the fitted
profile. It times the load without the icon, the drawing, and the next load
with the icon kept, and asserts the kept icon is reused.
## 2026-09-30 Steel Ball: real steel, minigame-only harm (for v0.1.10, branch `claude/project-thread-bya1ck`)

Max asked for the Steel Ball back, looking like real reflective steel
("UE5 like"). He also asked that it do harm only in minigames: it breaks
through builds, kills players and wrecks vehicles, including a tank hit by
a Gravity Gun throw. Outside minigames it is a heavy ball that pushes
vehicles aside, with no tumbling, no fake kills and no damage.

Behaviour (`bri-vehicles` schema, `session/movables.rs`, `session/vehicles.rs`):
- `harms_only_in_minigames` gates every harm on the source being in a
  minigame. It never harms its own thrower.
- `smash.energy_per_volume` punches through. The ball's ½mv² pays for bricks,
  nearest first (through `breakable_bricks`, the rocket rules), and it
  keeps what is left as speed. It shares `knock_out_bricks` with explosions.
- `smash.wreck_speed` damages vehicles by closing speed (the struck body's
  own velocity subtracted), from none at 14 to all their health at 26,
  under v20's minigame vehicle damage rule.
- A roll over a player bumps them as any vehicle does. It tumbles and
  damages them only when the minigame allows the hurt.
- Portals carry the ball with `VehiclesWorld::carry`, which keeps its id
  (owner, thrower credit) and turns its velocity and previous velocity.

Look (engine):
- `bri_content::shape::Metal` on a package material draws as
  `MaterialKind::Metal`: GGX with height-correlated Smith visibility,
  Schlick Fresnel, and Karis' split-sum environment term.
- It takes highlights from the sun (both lighting modes), map lamps and
  brick or player lights. Its detail texture holds roughness, cavity and
  tilt (a cotangent-frame normal).
- `bri_render::environment_probe` draws six 128² faces around the metal
  object nearest the player, within mirror distance, when Mirrors are on.
  Two faces are drawn per frame, or all six after it moves 6 units.
- The faces fold into a 256² octahedral map with 9 box-filtered mips, bound
  in the metal material's slot 2. The world shader already uses all 16
  texture bindings every GPU guarantees, and a cube binding broke that
  limit.
- Surfaces far from the probe, or all metal with Mirrors off, reflect a sky
  built from the map's fog and ambient colours.
- Every asset comes from `tools/make_steel_ball_assets.py` (nothing
  downloaded). `steel-ball-fx` is now sounds only; the steel look comes
  from the engine, not client code.

Shipping: the three Steel Ball Add-Ons are in `packages/default-addons.json`
turned off, like the other showcase Add-Ons. No protocol change.

Tests:
- `bri-sim --test showcase`: punch-through only in minigames; bumps outside
  them and kills inside them; vehicle wrecks only in minigames.
- `bri-render --test metal`: a mirror ball in a coloured room shows each
  wall on the right side, at 1x and 4x MSAA. With no probe it shows only
  the sky. The steel-ball scene is saved with `BRI_METAL_SHOT`.
- `bri-client` `metal_tests`, `bri-client-sandbox --test showcase`,
  `bri-package`, all of `bri-render`.
- Clippy on the changed crates is clean. This container's newer clippy
  also flags three lints in untouched code, which were left alone.

Environment consistency (Max: "need environmental consistency"): the
probe's faces draw everything a mirror's view does.
- Sprites and particles, plants, rain and snow, and Add-On world layers
  are each prepared per face (`EnvironmentProbe::face_views`).
- Mirror and portal surfaces show their echo or silver
  (`Reflections::prepare_view`).
- Bodies are built for every player while the probe draws, and so are the
  player's own items.
- The effects, foliage, weather and Add-On renderers now make any higher
  view on demand, since the probe's views come after the mirrors'.

More tests:
- `bri-fx-runtime --test metal_sprites`: a sprite behind the viewer shows
  in the ball, and without the probe's view of the effects it does not.
- `bri-render --test metal a_mirror_behind_the_viewer_shows_in_the_ball`:
  the ball shows the mirror's silver, not the yellow wall under it. It
  fails when the surfaces are not drawn in the probe.

Render: `/mnt/project-files/steel-ball/steel-ball-v2.png`. Max's in-game
check is a Steel Ball near bricks at Mirrors Medium, in Unified+Shine and
in Dynamic.

Gravity Gun wheel reels in play (2026-10-01, Max on v0.1.10: "gravity gun
scrolling still switches tool instead of letting me reel in or out"). Cause:
`App::follow_control` runs every frame and calls `Controls::follow`. For
`ControlObject::Player` that dropped the held trigger (meant only for coming
back from a camera), so `controls.held(Fire)` was false a frame after every
press. `update_held_weapon` therefore never gave the gun the wheel. The
v0.1.9 fix (`note_trigger`) was right but was undone every frame, and its
test checked only `note_trigger`. Fix: `follow(Player)` drops the trigger
only when it is coming back from a camera. Who sees the wheel is now decided
where the wheel is read. The app tells the UI only that the held tool has a
`wheel` command (and is not fired from a camera or a gunner's seat). The UI
gives that tool the wheel while its own `mouseFire` hold is down, so a press
and a roll in the same frame reach the tool, and nothing else sees the wheel
meanwhile. Tests: `app::tests::rolling_the_wheel_with_the_trigger_held_reels_and_never_switches_tools`
(the real UI with `mouseFire`/`scrollInventory` binds, and each frame run as
the game does: drain and note the trigger, follow control, claim the wheel;
it fails on 1b2747e4 with the trigger dropped), and
`runtime_input::wheel_goes_to_the_held_tool_while_it_takes_the_wheel` (no
trigger, scrolls; trigger held, reels in whole notches; released, scrolls).
