# Regenerating native content from a v20 install

Everything under `content/` is generated from an unmodified Blockland v20
folder and is never committed. One script rebuilds every pack the client loads,
in dependency order, on Windows or Linux:

```sh
python tools/regenerate_content.py --v20 "/path/to/Blockland v20"
```

The pack names come from `ContentConfig::default` in
`crates/client/src/content.rs`, so the output always matches what the client
loads. The run ends with `bri-client --check`, which validates every pack
without opening a window or audio device.

## Requirements

- Rust (stable, 1.93 or newer) and git.
- Python 3 with Pillow (`pip install pillow`) for the item presentation step.
- The v20 install: the folder with `base/`, `Add-Ons/` and `saves/`. It is only
  read. The designated reference is the B4v21 launcher's `versions/Blockland v20`.
- To decompile the v20 scripts, one of:
  - Windows: nothing extra. The script downloads the pinned `dso-sharp.exe`
    2.1.0 release and checks its SHA-256.
  - Linux (or any OS with it): a .NET 8 SDK (`pacman -S dotnet-sdk`,
    `apt install dotnet-sdk-8.0`). The script builds dso-sharp 2.1.0 from source.
- Network access for the two pinned downloads below and for cargo.

## What it does, in order

| Step | Output | Tool |
|---|---|---|
| `decompile` | `.research/v20-dso/` | dso-sharp 2.1.0 `-g blv20 -X` over copies of `base/**/*.dso` |
| | `.research/bl-decompiled/` | [bl-decompiled](https://github.com/Elletra/bl-decompiled) at `b519133` |
| `build_tools` | `target/release/` | `cargo build --release --locked` for every converter |
| `geometry` | maps-pass | `bri-convert` (terrain, bricks, shapes, animation, interiors, missions) |
| `brick_catalog` | stock-catalog | `stock_catalog` with the seven stock brick add-ons |
| `map_bundle` | map-bundle | `map_bundle` over every `Add-Ons/Map_*/*.mis`, lighting baked |
| `ui_pack` | ui-pack | `bri-ui-import` (needs the brick catalog for build-menu icons) |
| `brick_materials` | brick-materials | `brick_material_bundle` |
| `avatar` | avatar-pack | `avatar_bundle` (rig) then `avatar_material_bundle` |
| `effects` | effects-pass | `effect_bundle` |
| `weapons` | weapons-pack | `bri-weapons-import` |
| `weapon_debris` | weapon-debris-pack | `bri-weapon-debris-import` |
| `effects_runtime` | effects-runtime-pack | `bri-fx-import` (base) then `bri-weapon-effects-import` |
| `item_presentation` | item-presentation-pack | `docs/research/item-rendering/build_presentation.py` |
| `audio` | audio-pack | `bri-audio-import` |
| `vehicles` | vehicles-pack | `bri-vehicles-import` |
| `events` | events-pack | `crates/events-import/import_events.py` |
| `weather` | weather-pack | `bri-weather-import` (reads the map bundle) |
| `foliage` | foliage-pack | `bri-foliage-import` (reads the map bundle) |
| `worlds` | worlds-pass | `import_saves` over `saves/`, then `bind_world_events` |
| `tutorial` | tutorial-pack | Map_Tutorial's saves through `import_saves` and `bind_world_events`, then `tutorial_pack` |
| `check` | | `bri-client --check content` |

Intermediate outputs (the avatar rig, the base effects pack, unbound worlds)
go to `content/_regeneration/`.

Existing pack folders are kept, so a failed or interrupted run resumes where it
stopped. To rebuild a pack, delete its folder and everything built from it (see
the Tool column), then rerun. `--from <step>` and `--only <step>` narrow a run;
`--redecompile` redoes the decompile step.

The geometry pass always rejects two stock files on purpose (a brick fragment
that is not a standalone BLB, and a DTS v18 editor marker); the script accepts
exactly those and stops on any other conversion failure.

## Mission lighting

Map lighting is baked by `map_bundle` from the originals, a port of v20's
own sun lighting pass. It does not read `.ml` lighting caches, so the output
does not depend on which maps someone has played. Accuracy against the engine's
own caches is recorded in content-conversion.md ("Mission lighting bake").

## Reproducibility

On 2026-09-27 a run from an empty checkout against the reference install
produced these packs byte-for-byte identical to the shipped ones: map bundle,
stock catalog, effects, weapons, item presentation, vehicles, events and
tutorial. The re-decompiled scripts are identical to the existing
`.research/v20-dso`. Most remaining differences are provenance records
(recorded paths, source labels). The stock saves keep a "awaiting catalog
binding" diagnostic on a few raw event rows that the shipped pack cleared, and
the shipped geometry pass also holds conversions of extra add-ons from an older
secondary install, which the client does not need. The regenerated set passes
`bri-client --check`.
