# 2026-11-10 NewJeep: importer fixes and vehicle enhancement (branch `bionic/legacy-addon-import-fixes`)

Phase 1 made the legacy `Vehicle_NewJeep.zip` importable at all; phase 2
enhanced the imported package in place (package data only, no engine changes)
for a Renderman treasure-hunt survival server: headlights with cone beams,
brake/reverse lights, and steering wiring. Phases 3–4 added the engine sound,
steering-wheel grips, merged rear lights, horn and headlight clicks; the final
round re-packaged the result as one co-located add-on folder (the layout
`support_render` uses).

## Phase 1 — importer fixes (prior session, verified this round)

The CP1252 legacy-text fallback is documented in detail in
`docs/progress/2026-11-10-addon-import-cp1252.md`. Two further importer
fixes landed on the same branch to get NewJeep through:

### DTS material names with non-UTF-8 bytes (`crates/convert/src/shape.rs`)

The 2012 NewJeep pack leaves raw (non-UTF-8) bytes in a texture-less
material's name slot; Torque copies such names verbatim. `read_dts` now reads
the name strictly and, on failure, renames the material to `material_{i}`
with a provenance warning instead of failing the whole file. Unit test added
(a minimal DTS v24 with a raw-byte second material name). The fix is live in
the imported content: all three NewJeep body shapes carry a renamed
`material_5`/`material_6`.

### Wheel lowering from present hub nodes (`crates/vehicles-import/src/lib.rs`)

The importer previously iterated `0..numWheels` and hard-failed when a
declared `hub{i}` node was missing. NewJeep declares four wheels but has no
`hub2`, so every definition failed to import. The fix mirrors Torque's actual
behaviour (`WheeledVehicleData::preload` scans the eight `MaxWheels` slots and
compacts the ones it finds, wheeledVehicle.cc):

- Wheels are lowered from whichever of `hub0..hub7` exist; a missing hub
  drops that corner instead of failing.
- v20's onAdd steering/powered pattern applies to the compacted indices; a
  wheel past the declared count keeps Torque defaults (no steering, powered).
  Tank vehicles keep their per-slot pattern.
- A missing declared hub is recorded as an adaptation note.

All three NewJeep definitions carry: `Model lacks hub2; Torque skips that
wheel slot and compacts the rest (4 of the declared 4 wheels lowered)`. Unit
test added (`TestJeep` fixture with a missing `hub2`).

### Re-imports

- `content/render-pack/vehicle_newjeep` ← `.research/to port/Vehicle_NewJeep.zip`
  (package dir `newjeep-reimport-20261110b`; see its `IMPORT-REPORT.md`).
- The CP1252 re-imports (`brick_extraramps`, `brick_1randompack`) are in the
  doc above.

## Phase 2 — vehicle enhancement (this session)

Scope: package data only (`assets/vehicles.json` + three body shape files),
standalone — no runtime dependency on SpeedKart resources, no re-import.
Feature list approved by the user; design decisions below are the agent's.

### What was added to each of the three definitions

- `"steering_sequence": "steering"` — all bodies have a 12-frame `steering`
  clip (yaw on the empty `steering` node). Harmless no-op today (nothing is
  parented under that node), future-proof for a wheel model.
- `lights` (5 lamps): two warm-white headlight cones (`inner 20°, outer 50°`,
  radius 35) on the lens node, default switch activation; two red brake lamps
  (radius 8) at the rear corner boxes, active while occupied + braking; one
  white reverse lamp (radius 6) between the rear corners, active while
  occupied + reversing. Lamp offsets were measured from the geometry per
  body (identical across all three, verified).
- `visuals` (3 bindings, SpeedKart convention — `destroyed` under `none`):
  lens object black unless switched on; red brake object black unless
  braking while occupied; new rear object black unless reversing while
  occupied.

### Shape surgery per body

The authored "lens" mesh (240 verts, unlit `whitelight`) actually contains
both the front headlight/grille clusters (z < 0) and the two small white rear
corner boxes that are the reverse lights (z > 0). Split at z=0 — verified
clean: zero straddling triangles in all three bodies, no vertex on z=0:

- Front half (192 verts / 96 tris) replaces the lens mesh in place; it keeps
  the `u`/`bar` object and gets the headlight visual binding.
- Rear half (48 verts / 24 tris, the two reverse-light corner boxes) becomes
  a new appended mesh + object `u_rear`/`bar_rear` on the same node, with the
  reverse-light visual binding.
- Both detail entries' `object_count` bumped by one (newjeep/roof 11→12;
  offroad was already at 12 objects → 13). The body pass uses only the
  max-threshold non-collision detail (`detail0`, mesh slot 0), so the new
  object draws there and contributes nothing to the collision detail (mesh
  slot 1) — no physics change.

`assets[]` sha256 for the three body models updated in `vehicles.json`;
wheel/wreckage shapes untouched, `presentation.json` untouched (it covers
only wheels + wreckage).

### Verification

- `target/release/bri-client --check content`: exit 0, output byte-identical
  to a pre-edit baseline (226 pre-existing fallbacks elsewhere), zero NewJeep
  mentions. Caveat: `--check`'s health gate covers weapons/items/presentation,
  not vehicle packs — so additionally:
- `target/release/vehicle_probe assets/vehicles.json …`: full serde
  deserialization + `Pack::validate()` + `verify_assets()` checksum pass; all
  three definitions spawn in a physics world with occupants and run 1200
  ticks fault-free.
- Python-level equivalents of the remaining runtime checks: every rule of
  `Shape::validate` (frame layout, finiteness, index bounds, detail ranges),
  plus `BodyRig::load`'s admission (visual object names exist in the shape,
  steering clip exists by name, lamp nodes exist). All pass.

## Phase 3 — engine sound and steering-wheel grips (this session)

The two items left open after phase 2 (engine-sound source; steering-wheel
grips) were resolved by user decisions: re-use SpeedKart's jeep idle loop for
the engine sound, and add two hand-grip nodes to the `steering` node. Package data only again; standalone preserved (the WAV is
copied into the package — no runtime reference to `support_vehicle_speedkart`).
Built by a one-shot Python script
(`/home/surdavow/.lmstudio/scratchpads/yp/newjeep_grips_engine.py`, fixed this
session); pre-edit copies of all ten affected files sit in
`newjeep_backup_210557/` beside it for rollback.

### Engine sound

- `assets/sounds/newjeep_idle.wav` (208,918 bytes) copied from
  `.research/to port/Vehicle_SpeedKart/SpeedKartJeep_Idle.wav` (mono 44.1 kHz,
  ~2.4 s). Provenance note appended to `package.json`.
- `weapons.json` gains a top-level `sounds` map:
  `{"vehicle_newjeep:newjeep_idle": {"file": "sounds/newjeep_idle.wav",
  "looping": true}}` (inserted before `explosions`; the pack had no sounds
  section). Key is lowercase, as the client's runtime lookup requires.
- Each definition gets `"engine_sound": {"sound":
  "vehicle_newjeep:newjeep_idle"}` after its `lights`. Defaults apply:
  pitch 0.8→1.6 and volume 0.6→1.0 from idle to full speed — tuning is the
  user's playtest call.
- Note: `Pack::validate()` does **not** cross-check `engine_sound` keys
  against the weapons manifest; that resolution happens client-side at runtime
  (`pack_sounds.get(key.to_ascii_lowercase())`). The build script and this
  doc's audit cover it manually.
- `presentation.json`'s `weapons_sha256` re-pinned to the new file bytes
  (verified against actual bytes at load).

### Steering-wheel grips + visible wheel ring

The `steering` node was empty, so bare grip nodes would hold air. Each body
gains, in addition to the two requested grip nodes:

- **Grips**: empty nodes `grip_right`/`grip_left`, children of `steering`
  (node 0 in all three shapes at t=(-0.706, 1.386, -0.512)), local
  `[±0.194, −0.194, +0.04]` — on the rim centerline r≈0.275, ±45° below
  horizontal, just toward the driver of the rim's back face; identity
  rotation. Wired via `seats[control].hands = {"right_node": "grip_right",
  "left_node": "grip_left"}` on each definition's single control seat.
- **Wheel ring** (agent extension): one combined mesh — 16 box-segment rim
  (mid-radius 0.275, radial half-width 0.03 → edge-midpoint band 0.245–0.305,
  facet corners out to ~0.310; z-thickness ±0.025) plus a 0.13×0.13×0.05
  center cap — as object `wheel` on the `steering` node, material = the
  shape's `black`. Corners are duplicated per face (408 verts / 408 normals /
  204 tris) because the schema requires one normal per vertex; every triangle
  is wound to its outward face normal. The geometry probe showed the dash
  panel sits flush at z≈−0.5 with wheel center z=−0.512, so the ring mounts
  directly on the dash — no column needed.
- Per body: mesh appended (index 13 in newjeep/roof, 14 in offroad), object
  `wheel` appended, both details' `object_count` +1 (newjeep/roof 12→13,
  offroad 13→14; the collision detail skips the single-mesh object via its
  mesh slot — same as `u_rear`). All three cockpits are identical, and the
  `steering` clip's absolute ±90°-about-local-Z keyframes are identical in all
  three shapes, so behavior while driven matches.

### Hash updates

- Three body sha256s updated in `vehicles.json` `assets[]`
  (`7afca…→6ac757d2…`, `900d…→856bbb7f…`, `4a773…→b44677ff…`).
- `presentation.json`: only `weapons_sha256` changed. Its `models{}` covers
  just the wheel and wreckage shapes (untouched) — the bodies are not in it,
  so zero model replacements there is correct, not a miss.
- Build-script bug found while re-running: `os.path.splitext("…shape.json")[0]`
  leaves the `.shape` suffix on the stem, so the vehicles.json asset-hash
  update silently no-op'd (the script's own cross-check used the same broken
  extraction and passed vacuously). Fixed with a two-dot-aware `stem_of()`
  helper plus a new final assertion that every model asset hash matches disk.

### Verification

- Independent audit of **every** sha256 reference in the package (vehicles
  assets, presentation models/textures/weapons_sha256/item_physics_sha256,
  sound files): 40/40 consistent with on-disk bytes.
- `target/release/vehicle_probe`: exit 0; all three definitions spawn with
  occupants and run 1200 ticks fault-free (full serde + `Pack::validate()` +
  asset checksums). Cosmetic note: its "11 vehicle definitions" summary line
  is a hardcoded string in the probe's println, not a count of this pack.
- `target/release/bri-client --check content`: exit 0, zero NewJeep/render-pack
  mentions, fallback count exactly the 226 pre-existing ones.

## Phase 4 — rear-light merge, louder engine, horn + headlight clicks, grip reposition (later session)

Second user feedback round on the same package: reverse and brake lights should be
one red state with the **entire** rear lens bar changing colour; the engine sound is
too quiet; headlights need an on/off click (reuse SpeedKart's); add honking; fix the
Blockhead's twisted steering-wheel grip. Package data only, standalone preserved.

### `assets/vehicles.json`

- All three rear lamps merged into one activation:
  `{"when":{"all":["occupied"],"any":["brake","reverse"]}}`, colour
  `[0.7, 0.125, 0.1]` (radii kept: 8/8/6). Headlights untouched.
- The two separate rear visual bindings merged into ONE binding over both rear
  objects (`["g", "u_rear"]`, offroad `["bar0", "bar_rear"]`) with the first use of
  `ObjectVariant`'s **on/off** appearances: on = colour `[179,32,26,255]`,
  off = black. Appearance colours are applied as 1×1 SceneImages with
  `srgb:false` (linear, `crates/client/src/vehicles/body.rs`), so `[179,32,26]`
  is exactly the lamp's linear `[0.7,0.125,0.1]` — the whole bar turns red for
  either brake or reverse.
- `engine_sound` gains `"idle_volume": 1.5, "full_volume": 2.0` (schema range
  0–4; runtime gain cap 4).
- The three body sha256s in `assets[]` re-pinned again after the grip edit below.

### Sounds (copied into the package — no SpeedKart dependency at runtime)

From `content/surda-pack/support_vehicle_speedkart/assets/sounds/`:
`speedkart_headlight_on.wav → newjeep_headlight_on.wav`,
`speedkart_headlight_off.wav → newjeep_headlight_off.wav`,
`speedkart_buggy_horn.wav → newjeep_horn.wav`. `weapons.json` gains three sound
entries (`vehicle_newjeep:newjeep_horn/_headlight_on/_headlight_off`, volume 1.0,
non-looping, not local); `content.json` gains the matching four kind:sound entries
(idle was already listed). `presentation.json`'s `weapons_sha256` re-pinned to the
new file bytes — **any future edit of weapons.json must re-hash this** or the whole
add-on is left out with "item-physics.json does not match weapons.json".
- Horn loudness (user feedback, repackaging round): `newjeep_horn.wav` amplified
  ×2 (+6 dB; peak −8.1 → −2.1 dBFS, no clipping). Pack-sound manifest volume is
  schema-capped at 0–1 (`crates/weapons/src/lib.rs`, sound validation) and already
  maxed at 1.0, so the loudness had to come from the samples themselves — the
  SpeedKart source it was copied from peaks at the same −8.1 dBFS. No manifest or
  hash re-pinning needed: no pinned hash covers a sound WAV (the sha256 entries in
  `weapons.json` are legacy-source provenance, not runtime-verified).

### Companion rules pack (`vehicle_newjeep-rules`) — co-located add-on layout

The user asked to keep rules and assets in one package, and pointed out that
the engine already supports exactly that: a **co-located** add-on folder (one
authoring/distribution directory holding separately loaded content and host
rules), as `content/render-pack/support_render/` demonstrates. The earlier
sibling-folder approach (`render-pack/vehicle_newjeep-rules/`) was an interim
decision made before that layout was known; it has been replaced.

The contract, verified in engine code (no changes — this branch stays
importer-fixes-only):

- Wrapper `content/render-pack/vehicle_newjeep/` holds `addon.json`
  (`{"schema_version":1,"content":"content","server":"server"}`, byte-identical to
  support_render's) plus two component subfolders:
  - `content/`: the client/shared data package — `package.json` (id
    `vehicle_newjeep`, weapons+vehicles, so its derived side is Shared),
    `assets/`, and the import report pair.
  - `server/`: the host-rules package — `package.json` (id
    `vehicle_newjeep-rules`, hard dependency `{"vehicle_newjeep": "=1.0.0"}`,
    capabilities `[player, effects]`; behaviour+script only, so its derived side
    is exactly Server), `behaviour.json` (state `player.horn_at`; hooks
    `on_trigger`, `on_vehicle_lights`) and `newjeep.rhai`.
- Validation lives in `crates/package/src/addon.rs`
  (`Layout::parse/load/validate_manifests`): content component must be
  client-or-shared, server component must derive exactly Server, content must
  name the rules id in `companions`, and the rules manifest must depend on the
  content id. Both relationships were already true in our manifests (kept from
  the sibling approach), so **no manifest edits** were needed — only files moved.
- Sides are derived, never written: `PackageInfo::side()`
  (`crates/package/src/library.rs`) calls `side_for_package(provides kinds,
  code owner)`; a behaviour+script-only package is Server. The `"side"` field in
  the list files is advisory — `follow_manifest_sides` recomputes it from each
  manifest on every load.
- `content/packages.json` lists **one entry per component subfolder** (a wrapper
  folder cannot be listed: an entry resolves `<dir>/package.json`, and a
  co-located root holds only `addon.json`). The single old entry became:
  `{"id":"vehicle_newjeep",...,"side":"shared","dir":"render-pack/vehicle_newjeep/content"}`
  + `{"id":"vehicle_newjeep-rules",...,"side":"server","dir":"render-pack/vehicle_newjeep/server"}`.
- Companion auto-follow still works inside the wrapper: `Beside::folder(dir)`
  (`library.rs`) scans the *parent* of an entry dir, so for
  `…/vehicle_newjeep/content` that parent is the wrapper and both components are
  found by id; toggling NewJeep on/off in Add-Ons turns the rules with it.
- Sync semantics (proven by `crates/package-runtime/tests/co_located.rs`): the
  host catalog gets the behaviour, but a client catalog sees only the content
  package — the `.rhai` is never synced to joiners. The pair travels together on
  every load for both host and joiners (`PackageSet::load_root` follows
  companions).
- Behaviour: fire press honks at most once per 24 ticks while mounted in a NewJeep
  (nearest vehicle within 8.0 whose definition contains `":vehicle/newjeep"`),
  swallowing the trigger as SpeedKart did; the light key plays the headlight
  on/off click at the vehicle's position so nearby players hear it.

### Verification (repackaging round)

- `target/release/bri-client --check content`: exit 0. Health count back to the
  Phase-4 state: 20 problems, NewJeep contributing exactly its one pre-existing
  informational line ("rules unported scripts": the four legacy TorqueScript
  functions from the import; the rhai rules pack re-implements the driver
  behaviour natively). No "left out", no companion/dependency problems — their
  absence in `logs/add-on-health.json` proves both components load and see each
  other on.
- `target/release/vehicle_probe content/render-pack/vehicle_newjeep/content/assets/vehicles.json …`: exit 0 at the new path (full serde +
  `Pack::validate()` + asset checksums; all three definitions spawn with
  occupants, 1200 ticks fault-free).
- **`import-report.json` restored.** A previous verification round ran
  `vehicle_probe` with its *output* pointed at the package's
  `import-report.json`, silently replacing the importer's machine-readable
  report (with its four `needs_behaviour` entries) by a simulation summary —
  which is also why that health line had vanished from the check output until
  then. The original was restored byte-for-byte from the re-import staging folder
  (`newjeep-reimport-20261110b/`, whose `IMPORT-REPORT.md` matches the package's).
  Future probe runs must write their report somewhere else (e.g. /tmp).

### Grip reposition

Phase 3 placed the grips at `[±0.194, −0.194, +0.04]` (≈4/8 o'clock — the
twisted look in the user's screenshot). Repositioned to **`[±0.28, 0.0, +0.04]`
in all three body shapes**: on the wheel ring's rim centerline (r≈0.28) at
3/9 o'clock, just toward the driver of the wheel plane — a standard driving
posture matching SpeedKart Jeep's horizontal hand placement (`lhand16`/`rhand16`
at `±0.46, +0.08`). Exact string replacement; per file only the two grip nodes
changed (asserted node-by-node). The three body sha256s in vehicles.json's
`assets[]` re-pinned (`7afca…→91ff3244…`, `900d…→9b9c38a0…`,
`4a773…→ddea8645…`). `presentation.json` untouched — its `models{}` covers only
wheel + wreckage, and the grip edit touches neither.

### Verification (this round)

- `target/release/bri-client --check content`: exit 0. Diff vs the pre-change
  baseline is exactly: one informational health line for NewJeep ("rules unported
  scripts": 4 legacy TorqueScript functions — `NewJeepvehicle::onadd` et al. — have
  no native port; same class of warning as `vehicle_stunt_plane` and other imports;
  pre-existing gap from the import, not this round's work) and the health count
  19→20. No "left out", no `server_content`, no hash mismatches.
- The check compiles scripts both host-side (`Runtime::compile` +
  `unknown_calls()`) and player-side; zero diagnostics for either package, so
  every rhai API call in `newjeep.rhai` resolves in this build. Companion chain
  proven live: with the parent briefly broken (stale shape hash), the check reported
  "vehicle_newjeep-rules needs vehicle_newjeep, which is not enabled"; once fixed,
  it loads silently.
- `target/release/vehicle_probe`: exit 0 — full serde + `Pack::validate()` +
  asset checksums, all definitions spawn with occupants and run 1200 ticks
  fault-free.

## Phase 5 — middle rear lamp removal + grip fine-tune (this session)

Third user feedback round on the same package: drop only the middle red rear
lamp from each jeep definition (the two corner brake/reverse lamps stay, and
the two headlight spotlights were confirmed as-is), and fix the steering
grips — the user declined to tune them manually and left it to judgement.
Package data only; no engine changes on this branch.

### Middle rear lamp removal (`assets/vehicles.json`)

Phase 4's merge had left three rear lamps per definition: two corners (radius
8, merged brake/reverse activation) plus a middle red lamp at offset
`[0.0109, 1.0819, 2.78]` (radius 6 — node `bar0` in def 0, `u0` in defs 1–2).
Removed from all three definitions by matching the unique offset string; each
definition now has exactly **4 lamps**: two headlight cones (radius 35) + two
corner brake/reverse (radius 8). Nothing else touched.

### Grip reposition (three body shape files)

Phase 4's `[±0.28, 0.0, +0.04]` sat on the rim centerline dead at 9/3
o'clock; per the user's screenshot it still read too low and wide for a
natural hold. Repositioned to **`[±0.238, +0.138, +0.03]` in all three body
shapes** — ≈30° above horizontal on the rim centerline (r≈0.275;
x=±0.275·cos θ, y=+0.275·sin θ), i.e. a 10/2 o'clock driving posture with the
palm just ahead of the rim's front face (the rim band is ±0.025 thick in z).
Exact string replacement; asserted count==1 per node per file. The other three
shape files (wheel/wreckage) untouched.

Tuning note for future rounds: the grip nodes' rotation fields are irrelevant
— verified in `hand_holds` (`crates/client/src/vehicles.rs`, ~line 1407):
at rest the node's own rotation cancels out and only the steering delta is
applied; IK reach is position-only via `apply_body_fitting`
(`crates/content/src/body_pose.rs`). Only x/y/z need adjusting.

### Hash re-pins

- `vehicles.json` `assets[]`: `7afca…: 91ff3244…→24be39b7…`,
  `900d…: 9b9c38a0…→8cfdb405…`, `4a773…: ddea8645…→e7abbfa5…`.
- `presentation.json` untouched again — its `models{}` covers only wheel +
  wreckage, and this round touched neither.

### Verification

- `target/release/bri-client --check content`: exit 0, "Startup validation
  passed", health count still 20 (unchanged from Phase 4), zero "left out"
  lines. Output saved to `/tmp/bri_check_lamps.txt`.
- `target/release/vehicle_probe …/content/assets/vehicles.json
  /tmp/newjeep_probe_lamps.json`: exit 0, 1200 ticks fault-free (probe report
  written outside the package — see Phase 4's import-report clobber incident).
- Full package hash audit: **39 consistent, 0 mismatched** (vehicles
  `assets[]`, presentation pins for item-physics/weapons + models, weapons
  sounds).

## Phase 6 — grip spread + straighten (this session)

Fourth user feedback round: the hands read "almost better" but still pinched
inward instead of wrapping the rim — spread them a little more and straighten
them so they actually grab the wheel.

Re-read the IK before choosing values: `hand_holds`
(`crates/client/src/vehicles.rs`) emits position + steering-delta only, and
`apply_body_fitting` (`crates/content/src/body_pose.rs`) resolves it with
two-bone positional IK in `crate::hold::reach`, which rotates the whole arm
chain to point shoulder→target. The clawed look was therefore a target-
placement problem: grips too close to the body centerline fold the arms
inward. Moving them further out along the rim straightens the reach.

### New grip values (three body shape files)

**`[±0.257, +0.099, +0.03]`** — still on the rim centerline (r≈0.275) but at
≈21° above horizontal instead of ≈30°: total hand spread widens 0.476→0.514,
height drops toward the natural 9/3-o'clock grab height, z unchanged so the
palm still wraps the rim's front face (band ±0.025 thick). Exact string
replacement; asserted one occurrence per node per file before replacing.

### Hash re-pins

- `vehicles.json` `assets[]`: `7afca…: 24be39b7…→d57b352f…`,
  `900d…: 8cfdb405…→3ffb26e9…`, `4a773…: e7abbfa5…→a4c92e00…`.
- `presentation.json` untouched again (wheel + wreckage only).

### Verification

- `target/release/bri-client --check content`: exit 0, "Startup validation
  passed". Note: the user was reorganising add-ons on their laptop at this
  moment and the live sync briefly dropped other packages from
  `content/packages.json` (map count 23→21 as `gsfsamain`/`gsfsanight`
  moved); unrelated to this change. NewJeep's own files verified intact
  afterwards.
- `target/release/vehicle_probe … /tmp/newjeep_probe_grips2.json`: exit 0,
  1200 ticks fault-free (report outside the package, per Phase 4).
- Post-sync re-audit: all three bodies carry only the new grip values
  (zero stale), every `assets[]` hash matches disk, lamps still
  `[35, 35, 8, 8]` per definition.

## Not done / open

- In-game playtest (light intensity, cone angles, engine-sound tuning,
  grip feel) is the user's; nothing here has been committed — committing is
  the user's call.
- The visible steering-wheel ring mesh is the agent's extension beyond the
  two requested grip nodes. To veto it: drop object `wheel` and its mesh from
  each body shape, decrement both details' `object_count` by one, re-hash the
  three shapes in `vehicles.json` (the grips themselves stay).
- Left/right hand assignment is visually moot on a symmetric wheel; if a
  playtest shows mirroring, swap the two node names in `vehicles.json`
  (`hands.right_node`/`left_node`) — no re-hash needed.
- Untracked scratch file `crates/convert/examples/dts_probe.rs` from the DTS
  investigation: delete or keep is an open decision (kept as a dev tool).

## State of the branch

Importer fixes (`crates/addon-import/src/{lib,map_audio,reference,v20_map}.rs`,
`crates/convert/data/blb-repairs.json`, `crates/convert/src/shape.rs`,
`crates/vehicles-import/src/lib.rs`) and this doc are committed on
`bionic/legacy-addon-import-fixes`; only untracked `crates/convert/examples/`
remains. All content changes live under gitignored `content/`.
