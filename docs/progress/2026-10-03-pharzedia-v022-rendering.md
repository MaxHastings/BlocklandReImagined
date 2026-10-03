# 2026-10-03 Pharzedia v0.2.2 rendering handoff

Scope: Maxwell requires both the duplicator preview crash fix and independently
modern Dynamic lighting in v0.2.2. Source changes are isolated on
`codex/pharzedia-v022`, based on candidate `abcb22e1275d19b6da5bd7458c63b6728694d30f`,
in the reused `content-reload` worktree. Root owns integration with the dirty
candidate, content regeneration, gate, platform checks and publication. No
original installation was modified and no interactive playtest was performed.
The full alpha contract remains open.

## Definition of done for these two source work items

- [x] Trace the reported fatal preview budget error and bound cosmetic preview
  geometry without dropping authoritative copied/plantable bricks.
- [x] Dynamic startup, shading and shadows do not require baked illumination,
  baked sun visibility, residual volumes, per-texel shares or lightmap cleanup.
- [x] Recover only source light descriptors offline, with honest modern fallback
  when descriptors are missing or stale; no runtime baked-light blending.
- [x] Preserve Classic/Unified intended lighting and isolate their preparation.
- [x] Compile, strict lint, source regressions and offscreen checks pass within
  this lane's scope; the two unrelated candidate-baseline client failures are
  explicitly accounted for below.
- [ ] Root integrate the patch, regenerate/package the descriptor sidecar and
  enforce its presence/validity in packaged `bri-client --check`.
- [ ] Full gate, Windows CI, cross-platform packaging and Maxwell's acceptance.

## Preview crash: cause and correction

The attached `client-20261003-185830.stderr.log` reports the exact flat-scene
builder error “World requires 100024 or more brick triangles … budget 100000”.
That wording is unique to `world_scene`, called with 100,000 for the shared copy
preview used by both duplicators. The authoritative chunked world has a separate
64-million triangle budget and different error text. The cosmetic preview's
error propagated out of rendering and terminated the client.

`build_placement_preview` validates all ghost bricks and preflights their triangle
count. Up to 200,000 source triangles it draws the complete preview. Above that
finite budget it draws one enclosing shell for the whole selection and explicitly
announces that representation in chat/log; the entire blueprint remains available
for planting. The replicated 10,000 ghost-brick limit is unchanged. The ordinary
world builder still rejects excess geometry rather than truncating the world.
Two regression cases cover 100,032 triangles crossing the reported old threshold
and a complex 10,000-brick selection exceeding the new preview budget. Temporary
outer/inner shells are bounded to 57.6 MB vertices plus 4.8 MB indices at the full
preview limit, apart from materials and allocation overhead.

## Lighting architecture

| Mode | Runtime source/preparation | Final illumination |
|---|---|---|
| Classic | Existing authored illumination images, decomposition, compatibility volume and leak patches | Existing legacy appearance and projected-shadow behavior |
| Unified | Existing recovered fit, visibility/residual volume, leak patches and switchable per-texel shares | Existing legacy map/object model, including exact switched-light shares |
| Dynamic | Modern native loader and prepared typed source-light sidecar; no compatibility worker | Current sun/ambient, recovered live lamps, runtime point lights and current geometry shadow maps |

`load_map_bundle_dynamic` does not decode embedded illumination PNGs or open
mission interior/terrain illumination images. It binds white unused slots and
retains ordinary diffuse/detail textures, terrain material weights and water
shore/depth masks. Shared native interior JSON still contains historical encoded
lightmap bytes; these are parsed as asset data, never decoded or shaded. Dynamic
also bypasses terrain emboss offsets tied to the old baked sun; ordinary terrain
detail remains. A regression corrupts embedded PNG bytes and removes declared
mission illumination files: Dynamic loads, while the compatibility loader fails
as expected. Runtime mode source reloads are off-thread and latest-selection wins.
A modern source stays in modern shading until a compatibility scene has loaded;
failed preparation logs an error and retains the valid current source.

`lighting-parameters.json` schema 1 contains map IDs and only light position,
color and inner/outer falloff radii. Validation enforces at most 24 lights/map,
finite nonnegative colors and valid radii, a bounded 256 KiB read, and the SHA256
of `bundle.json`. That metadata names content-addressed source assets and their
placements; validation never opens baked images. Missing/stale/invalid parameters
produce an explicit live sun/ambient-only diagnostic, not a legacy fallback.

`prepare_lighting <native-bundle> [output] [--check | --force]` is offline and
needs no GPU or original installation access. A valid complete default sidecar
is checked and skipped; `--force` recovers again. `--check` validates the bundle's
own sidecar and every map entry, failing without recovery. Root can call
`Parameters::read(bundle)?.lights(map_id)?` directly from packaged content checks.
Recovery reuses deterministic inverse fitting of original interior lightmaps;
no authored light positions survived the converted source. These are estimates,
not exact recovered authored lights: broad bounced-fill terms and fit error can
influence estimated positions/colors/radii. The modern Lambert shading model
intentionally does not preserve the old no-cosine fitted lightmap appearance.

Dynamic has a distinct descriptor-only GPU entry point that clears the legacy
volume and never uploads its visibility grid. Shared material shaders dispatch
modern surface, vertex-lit, terrain, water and metal lighting before legacy
illumination reads. Poisoned legacy images, visibility/residual data and baked
environment uniforms cannot affect the tested modern output. Shadows-off keeps
mode 3 and the live lighting model; it no longer silently changes to Unified.
Bricks/debris cast in Dynamic independently of the compatibility Brick Shadows
preference. Terrain is included in modern geometry shadow passes. Cached map
shadow keys include hidden batch ranges and instance transforms, so removed map
geometry stops shadowing. Reflection/probe caches are invalidated when source or
bound lighting mode changes, preventing old-mode illumination from reaching metal.

Classic and Unified retain required decomposition, cleanup and switchable shares.
The compatibility client bake stops constructing the unused all-lights residual
input. A deterministic test compares its lights, visibility, residual, leak fixes
and switchable sheets exactly against the prior staged preparation. Historical
cache fields/diagnostic APIs remain deliberately intact. The old `lighting_probe`
now explicitly supports Classic/Unified only and rejects its obsolete
`BRI_DYNAMIC=1` legacy-preparation route; the independent modern all-map test is
the authoritative native Dynamic probe.

## Commands and evidence

Final validation on Apple M1 Pro (16 GPU cores), macOS Metal:

- `cargo build -p bri-client --bin bri-client`: passed.
- `cargo clippy -p bri-render -p bri-client --all-targets -- -D warnings`: passed.
- `cargo test -p bri-render --lib --tests -- --test-threads=1`: 114 passed,
  9 content-dependent tests ignored. Includes 16 lighting offscreen regressions,
  shader validation, compatibility preparation equivalence, current-geometry
  lamp/sun shadows, broken geometry, mode switching and water day/night behavior.
- `cargo test -p bri-client --lib -- --skip colorsets::tests::non_utf8_filenames_cannot_alias_a_selectable_palette --skip mirrors::tests::knocked_out_mirror_bricks_reflect_on_their_debris_until_it_fades`:
  408 passed, 55 ignored, 2 baseline tests filtered. Includes preview budget and
  modern preparation/latest source selection cases. The earlier unfiltered run
  had exactly those two failures (406 passed before the two source-state tests);
  root's existing dirty candidate fixes them. This lane does not claim the whole
  candidate gate passes or modify those unrelated files.
- `cargo run -p bri-render --bin prepare_lighting -- /tmp/bri-modern-map-bundle --check`:
  validated all 14 prepared map entries. Repeating without flags skipped recovery.
  Six negative sidecar checks (missing, stale hash, unsupported schema, missing map,
  invalid radius, oversized file) failed; valid data passed.
- `BRI_MODERN_BUNDLE=/tmp/bri-modern-map-bundle cargo test -p bri-render --test unified_lighting modern_dynamic_real_maps_without_legacy_preparation -- --ignored --nocapture --test-threads=1`:
  passed all 14 native maps, descriptor-only binding, no legacy preparation, actual
  instanced terrain, 960x540 captures, Best shadows. Six warmup frames precede 16
  submitted/waited frames/map. M1 Pro wall-time medians ranged 1.53–9.50 ms;
  Slate Desert had a 43.07 ms maximum. Concurrent unrelated build/test work may
  affect these short measurements. This is bounded correctness inspection, not
  sustained frame pacing, GPU timestamp benchmarking, Windows acceptance or the
  million-brick performance target.

Private source receipt: bundle metadata SHA256
`b5f6d8d59051106d3b649c862b5b46e5b0ceb02601b90b973eae722582ddac70`.
Generated descriptor receipt: 27,942 bytes, SHA256
`106c02cf5acf807e285269559ffc643dfb4dfb9d5904aa68372914e86387adaa`.
Bedroom/BedroomDark/Tutorial have 24 estimated lights each, Kitchen/KitchenDark
19 each, the remaining nine maps zero. Generation took about 36 seconds total.
The staging bundle used read-only hardlinks to native content plus a separate
sidecar; the primary content pack was not modified.

Logs, sidecar receipt, all 14 PNGs and visually inspected contact sheet are retained
under ignored `artifacts/pharzedia-v022/` in this lane. The contact sheet shows
recognizable Bedroom/Kitchen/Tutorial geometry, terrain and water without obvious
corruption. Dark map views are very dark; Destruct is black with its zero live
illumination. Earlier arbitrary brightness assertions failed BedroomDark/Destruct;
they were replaced with a diagnostic-clear coverage assertion because brightness
is not a startup/shader validity condition. No baked fallback was added. This
inspection does not approve lighting fidelity or claim all map viewpoints work.
Early lint failures (test slice clones/ranges and source receiver type complexity)
were corrected; final strict lint passes.

## Explicit rendering limits and next work

Dynamic uses authored live ambient/shadow ambient for unreconstructed indirect
light; there is no GI or restored baked residual. Source light recovery is an
approximation and dark interiors need Maxwell's visual acceptance. Future typed
source light/emissive/spot metadata should replace inference where recoverable.

Sun shadows have finite cascades (Best 320 units); outside their range visibility
fades to unshadowed, with no baked-sun fallback. Every recovered map light has a
current static-map geometry cube (Best 512px faces, other enabled levels at least
256px). Cube refresh is bounded to 24 faces/frame, up to six frames for 24 lights;
no baked result fills that interval. Moving/brick lamp shadow slots are still
quality bounded (Best four, High two, Medium one, Low zero); runtime point lights
retain the existing bounded light grid and are not all separately shadow mapped.
Modern terrain casts/receives geometry shadows, but this does not implement general
terrain normal mapping or unlimited-distance shadowing. At Best the depth atlas is
31 x 2048² x 4 bytes = 496 MiB (96 MiB more than the previous Dynamic atlas), before
other rendering resources. Preview and shadow limits are explicit resource budgets,
not claims of universal 60 fps performance.

Root must preserve its dirty NPC/menu/body/naN fixes when integrating the small
app load signatures (`mod.rs`, `session.rs`, `net_events.rs`) and shared render
hunks. Add offline preparation after map generation in bootstrap/regeneration,
validate prepared sidecars in packaged checks, regenerate private content and
receipts, then run combined gate/Windows CI and package. Original game content,
private sidecars and captures are not committed. Maxwell alone performs the
interactive copy/plant, mode-switch, dark-room/light-breaking and sun/environment
acceptance checks. Publication remains the coordinator's authorized work.
