# Effects conversion and native bindings

## Implemented evidence — 2026-09-26

`bri-content::effects` is the native light/flare/particle/emitter schema. It
contains validated authored parameters and sampled curves, not Torque readers.
The converter follows bounded literal includes in source order, resolves literal
inheritance and indexed fields, supports the required uiName overrides, and
reports skipped conditional declarations and unknown fields. It never executes
TorqueScript. Source/effective declarations remain separate provenance.

Canonical conversion: `content/effects-pass-004`: 13 lights,119 particles,
120 emitters (102 with UI names),18 original texture files,76 source hashes,
zero conversion errors. Texture filenames are hashes; the original PNG/JPEG
bytes are unchanged. Reports retain ignored fields rather than silently claiming
that they run. Glow-paint alpha2 required widening the native HDR color bound;
it is preserved instead of clamped to1.

```
cargo run -p bri-convert --release --bin effect_bundle -- "<v20-root>" .research/v20-dso/server/scripts/allGameScripts-Vanilla.cs docs/vanilla-inventory.json content/effects-pass-004
cargo run -p bri-convert --release --bin import_saves -- "<v20-root>/saves" content/stock-catalog-004/stock-catalog.json content/worlds-pass-004 content/effects-pass-004/effects.json
```

Use new output paths when repeating these commands. Effect binding resolves all
453 light/emitter references in35 imported saves/276,612 bricks. An independent
Python comparison (`tools/verify_effect_bindings.py`) verifies only those
references changed from the already verified worlds-pass-003, all source BLS
bytes remain exact, and all18 packaged texture hashes match. Its report is
`content/worlds-pass-004/effect-binding-verification.json`.

## Important recovered behaviors

- Light_Basic uses an include whose filename casing differs; lookup is
  case-insensitive and rejects ambiguity. Colored lights inherit RedLight.
- Animated lights retain color, brightness and radius A–Z key curves,
  interpolation and periods. Authored units are retained.
- Particle_Player/Tools/FX_Cans name core emitters through assignments; an
  add-on declaration-only scan misses these wrench entries.
- Particle keys, lifetime variance, gravity/drag/inherited velocity, spin,
  emission distributions, blend modes and node time scales are native data.
  Unsupported animated texture/position/rotation features are explicit.

## Remaining limits

No native particle simulation, dynamic-light render adapter, flare renderer or
weapon/vehicle/player attachments are complete yet. Script-generated paint
variants, explosion/projectile relationships and callbacks need native adapters.
Three conditional declarations in Projectile_GravityRocket are diagnosed and
skipped. The later stock Rocket Launcher provides the referenced rocket trail;
exact conditional fallback ordering still needs behavior coverage.

Unknown/inert misspelled fields remain visible in provenance. The stock spawn
color has a surplus component: conversion takes the first four and reports it;
exact Blockland parser behavior is not independently established. OpenMBG's blue
light animation code appears to reuse green keys; the native schema uses authored
blue keys. Verify against Blockland's fork before asserting exact animation fidelity.

Defaults/onAdd constraints were checked against pinned references:
OpenMBU `3d6516e1c9cb43e61aead3369d1f7210d08b83ef`,
`engine/source/game/fx/particle.cpp` and `particleEmitter.cpp`, and OpenMBG
`9c5673f9a1c26348da445bb1dbfd88bf1ed3c3b7`, `fxLight.cc`.
These related engine versions support interpretation, not proof of Blockland's
modified engine. Files are in ignored `.research/openmbu-reference/`.

Parsing and binding are conversion evidence only. Full vanilla acceptance still
requires renderer/simulation integration and Maxwell's eventual playtest.
