# Native precipitation evidence and implementation

Date: 2026-09-26. Source installation was read-only at `E:/Downloads/B4v21Launcher/versions/Blockland v20`. No visible window, interactive input, playback or original writes were used. Runtime and importer are isolated in their own workspaces; shared manifests and existing FX crates were not edited.

## Converted scope

`content/weather-pack-001/weather.json` is native schema 1: two definitions, two map placements, 5,500 authored drops, three native RGBA PNG textures and thirteen provenance records. Eight records are primary original files: five images, two missions and `rain.cs`; the rest are recovered core script, native bundle/two scenes and an explicit atlas adaptation. Raw sources are retained byte-identically as content-addressed `.source` proof files. Runtime loads only JSON/PNG.

| Placement | Authored settings | Native result |
|---|---|---|
| Slopes, `SnowA`, 500 drops | Speed 0.5–1 per tick, mass .75–.85, 100×100×100 camera volume, turbulence .1 / .2 per tick, true billboards, radius .25 | Original snow alpha; four complete 2×2 atlas cells; no invented splash texture |
| Slate Storm Revised, `HeavyRain`, 5,000 drops | Speed 1.5–2 per tick, mass .75–.85, 200×100×200 camera volume, turbulence disabled, camera-velocity-sensitive billboards, radius .75 | Original rain 4×4 and splash 2×2 atlas; splash radius .2, 250 ms lifecycle |

Stable map IDs are `v20/add-ons/map_slopes/slopes.mis` and `v20/add-ons/map_slate_storm_revised/slatestormrevised.mis`. Definition IDs are `v20/weather/snowa` and `v20/weather/heavyrain`. Native Y-up conversion is `(x,z,-y)`. The converter compares precipitation fields and sky wind against original mission literals, checks native scene identity/schema/translation and retains original datatype fields separately in `datatype-fields.json`.

This is the precipitation scope found in the currently converted map bundle. Slate Storm Revised provenance is named explicitly; this report does not relabel every installed/community map as verified stock vanilla or declare full map acceptance complete.

## What is direct evidence, and what is adapted

Recovered `allGameScripts-Vanilla.cs` around line 19113 declares `SnowA`: snow texture, .25 drop size, .2 splash size, true billboards and 250 splash milliseconds. It declares no splash texture or weather sound. Original `Map_Slate_Storm_Revised.zip/rain.cs` declares the rain and splash texture references and HeavyRain sizes/mode/lifetime. Both original missions declare the counts, motion/mass/turbulence and volume values above.

The modern engine-family reference is OpenMBU commit `3d6516e1c9cb43e61aead3369d1f7210d08b83ef`, locally `.research/openmbu-reference/openmbu-precipitation.cpp`. It supports camera-local wrapping, random speed/mass/atlas cells, upstream collision cutoffs, wind/mass motion, non-accumulating sinusoidal turbulence, camera velocity billboard adjustment and animated splash frames. Some fields moved between datatype and instance in that later engine; it is **corroborating family evidence, not the closed v20 implementation**. The older `mbg-precipitation.cc` is insufficient for SnowA's modern fields.

The family GameBase source defines a 32 ms tick and precipitation advances speed once per tick. The schema therefore preserves authored per-tick values and names `legacy_tick_seconds` explicitly; host velocity uses units/second. Modern precipitation negates sky wind before applying wind/mass, so `reference_wind_velocity` contains negative native sky wind divided by .032. The runtime never silently chooses it: the host passes `WeatherEnvironment`. Exact v20 timing/sign remains a comparison item. [Primary GameBase source](https://raw.githubusercontent.com/MBU-Team/OpenMBU/3d6516e1c9cb43e61aead3369d1f7210d08b83ef/engine/source/game/gameBase.h).

The primary Torque-family texture loader joins `.jpg` with matching grayscale `.alpha.jpg` by copying grayscale bytes directly into alpha. The importer follows this without inversion, contrast normalization or premultiplication. The actual rain alpha range is 0–51; splash is 0–147. Snow preserves original PNG RGBA exactly. [Primary JPEG-alpha loader](https://raw.githubusercontent.com/MBU-Team/OpenMBG/9c5673f9a1c26348da445bb1dbfd88bf1ed3c3b7/engine/source/dgl/gTexManager.cc).

**Snow atlas override:** the actual 256×256 source contains four whole flakes in 2×2, with empty alpha seams at x/y 127–128. Applying later OpenMBU's generic 4×4 default would cut each snowflake into quarters. `crates/weather-import/atlas-adaptations.json` explicitly selects two cells per side and is copied/hashed into provenance. This is an evidence-based adaptation requiring exact v20 comparison, not a recovered declaration. Rain's 4×4 and splash's 2×2 follow actual image structure plus family defaults.

## Implemented behavior and tests

`WeatherWorld` implements seeded camera-local distribution, authored speed/mass, explicit wind, turbulent render displacement, wrapping, density, true and velocity-aligned billboards, atlas selection, roof occlusion, solid/water impacts and timed animated splashes. A host callback supplies closest collision results; no secondary physics world is created. Collision revision changes and wind changes invalidate cutoffs; teleports rebuild the volume. Pending/invalid queries are hidden. Large cosmetic time jumps have bounded catch-up and expose skipped seconds. Teardown clears sources, drops, splashes and caches.

GPU rendering takes host Device/Queue, camera matrix, formats/sample count and a render pass sharing scene depth. Original texels are uploaded into a bounded texture array without resizing. Back-to-front instances draw once, with alpha blend, 1/255 discard and read-only depth. The renderer owns no window or device. Host geometry/water collision integration and settings UI remain root integration work.

Validation completed:

- Nine CPU fixture tests: roofs/world revision, water splash lifecycle, partitioned timesteps, query budget/teleport, invalid callbacks, snow atlas/billboards, explicit wind/speed, density/stall/teardown and corrupt/unsafe/dimension-mismatched native resources.
- One GPU readback test: atlas selection, correct source alpha, occlusion by host depth and no depth writes by weather.
- One private actual-pack test, explicitly run: both definitions, all 5,500 authored drops, actual PNG alpha and impact lifecycle.
- One importer literal/datatype test; both crates pass Clippy with warnings denied.
- Independent Python/Pillow verification compares all thirteen source copies against separate inputs and verifies native encoded/decoded hashes. PNG snow pixels match exactly. JPEG decoder differences versus Pillow are at most 3/255 RGB and 1/255 alpha; original compressed source bytes remain exact. This is reported instead of claiming byte-identical JPEG decoder output.
- A second conversion into the importer's ignored target directory reproduces the pack files exactly.

Offscreen artifacts: `artifacts/native-weather/index.html`, four PNGs, `report.json`, `independent-verification.json` and `reproducibility.json`. On the recorded release run, 5,000 rain drops over 600 frames took 0.1237 ms median / 0.1375 ms p95 simulation; 3,375 visible drop/splash instances used one draw and 270,064 upload bytes. These are CPU timings with analytic roof/water callbacks, **not game FPS or measured Rapier/GPU completion timings**. Source atlas inspection confirms whole snowflakes and intentionally faint rain/splash alpha.

## Reproduce

The importer requires a fresh output path, canonicalizes it and rejects aliases/descendants of the source installation. Reads, archive entries, image dimensions and decoded budgets are bounded. Generated content remains ignored and original proof bytes must not be committed.

```powershell
cargo run --manifest-path crates/weather-import/Cargo.toml -- 'E:/Downloads/B4v21Launcher/versions/Blockland v20' .research/v20-dso/server/scripts/allGameScripts-Vanilla.cs content/map-bundle-014 content/weather-pack-001
python crates/weather-import/verify_pack.py content/weather-pack-001 'E:/Downloads/B4v21Launcher/versions/Blockland v20' .research/v20-dso/server/scripts/allGameScripts-Vanilla.cs content/map-bundle-014 artifacts/native-weather/independent-verification.json
cargo test --manifest-path crates/weather/Cargo.toml
cargo test --manifest-path crates/weather/Cargo.toml --test weather -- --ignored
cargo test --manifest-path crates/weather-import/Cargo.toml
cargo clippy --manifest-path crates/weather/Cargo.toml --all-targets -- -D warnings
cargo clippy --manifest-path crates/weather-import/Cargo.toml --all-targets -- -D warnings
cargo run --release --manifest-path crates/weather/Cargo.toml --example offscreen_weather -- content/weather-pack-001 artifacts/native-weather
```

## Remaining comparisons and integration obligations

Exact closed-v20 default atlas/tick/wind behavior and display-space brightness need Maxwell's original/native comparison; sRGB-correct host rendering is implemented, not asserted pixel-identical to old fixed-function output. Generic unsupported precipitation declarations stop conversion with an error. Weather audio remains host/audio-owned; neither converted datatype declares a sound. Fog/light tinting beyond the authored white particle modulation is not inferred. The host must combine solids and water for closest-hit queries, invalidate moving/edited roofs and choose explicit environment/settings. The gallery's synthetic planes prove the rendering/collision contract, not complete original map presentation. No alpha acceptance checkbox is closed by this subsystem alone.
