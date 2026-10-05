# Regenerating native content from a v20 install

Everything under `content/` is generated from an unmodified Blockland v20
folder and is never committed. From a fresh clone on Windows (Linux works for
cloud and CI tooling only; the game ships for Windows):

```sh
python tools/bootstrap.py --v20 "/path/to/Blockland v20"
```

`bootstrap.py` checks the toolchain first and prints the exact install command
for anything missing, then runs `tools/regenerate_content.py`, which rebuilds
every pack the client loads in dependency order and ends with
`bri-client --check` (it validates every pack without opening a window or
audio device). The v20 path is remembered in `content/_regeneration/`, so later
runs need no arguments; `BRI_V20` also works.

The pack names come from the base package list,
`crates/package/base-packages.json`, so the output always matches what the
client loads. When the client's `--check` reports missing packs, rerunning bootstrap
builds exactly those.

Platform status: the Windows path is verified from a fresh clone. The Linux
path (prerequisite hints, building dso-sharp with a .NET SDK) is for cloud and
CI tooling and is best effort.

## Requirements

`bootstrap.py --prerequisites` checks these without building anything.

- Rust (stable, 1.93 or newer, from rustup) and git.
- Python 3.9+ with Pillow (`python -m pip install pillow`) for item presentation.
- Linux (tooling only): a C compiler, pkg-config and the ALSA and udev headers
  (`apt install build-essential pkg-config libasound2-dev libudev-dev`, `pacman
  -S base-devel alsa-lib`).
- The v20 install: the folder with `base/`, `Add-Ons/` and `saves/`. It is only
  read. The designated reference is the B4v21 launcher's `versions/Blockland v20`.
- To decompile the v20 scripts (once per checkout):
  - Windows: nothing extra. The script downloads the pinned `dso-sharp.exe`
    2.1.0 release and checks its SHA-256.
  - Linux: any .NET 8 or newer SDK (`apt install dotnet-sdk-8.0`, `pacman -S
    dotnet-sdk`). The script fetches
    dso-sharp at the 2.1.0 commit and builds it framework-dependent with
    `dotnet build -p:PublishAot=false -p:RollForward=Major`, so no native AOT
    toolchain is needed and a newer SDK runs it. Do not run `dso-sharp --help`
    by hand: outside `-X` command-line mode it waits for a key press.
- Network access for the two pinned downloads below and for cargo.

## What it does, in order

| Step | Output | Tool |
|---|---|---|
| `decompile` | `.research/v20-dso/` | dso-sharp 2.1.0 `-g blv20 -X` over copies of `base/**/*.dso` |
| | `.research/bl-decompiled/` | [bl-decompiled](https://github.com/Elletra/bl-decompiled) at `b519133` |
| `build_tools` | `target/release/` | `cargo build --release --locked` for every workspace converter and `bri-client` |
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
| `worlds` | worlds-pass | `import_saves` over v20's `saves/` and, with `--bundled`, the repository's own `saves/<Map>/` builds (copied in unchanged; Load Bricks lists them as "Bundled build"), then `bind_world_events` |
| `tutorial` | tutorial-pack | Map_Tutorial's saves through `import_saves` and `bind_world_events`, then `tutorial_pack` |
| `check` | | `bri-client --check content` |

`item_presentation` also reads the UI and avatar packs, and the
`weapon_debris` and `weapon-effects` importers live outside the workspace with
their own `Cargo.lock`, built with `--locked`. Intermediate outputs (the avatar
rig, the base effects pack, unbound worlds) go to `content/_regeneration/`.

## Default Add-Ons

The default Add-Ons are on in every copy of the game until a player turns
them off: the Duplicator (two packages), the Stunt Plane (Kaje and
Ephialtes, a community Add-On that is not in the v20 install; Max approved
shipping it on 2026-09-28) and the Mirror (a 1x4x5 mirror built on the base
game's window brick; it holds only its catalog entry, and borrows the
window's shape and icon when the game loads). `packages/default-addons.json` lists them in load
order, and each is committed under `packages/<path>`. They are not generated
and need no v20 install.

A checkout's `content/` gets them when the game starts: each
is copied to `content/addons/<id>` when missing or different from the
checkout's copy. `bri-client --check` (bootstrap's last step, and the push
gate's content check over the shared main checkout) and `bri-server` change
nothing unless `BRI_INSTALL_DEFAULT_ADD_ONS=1` is set
(`bri_package::defaults::install_when_asked`). A release's content already
has them.
With no `content/packages.json`, the game loads the base game's list and the
default Add-Ons installed there, the list a release ships; nothing is
written to `packages.json`, so the checkout keeps following
`crates/package/base-packages.json`. A `packages.json` of your own (the
Add-Ons screen writes one) keeps your choices: a default you turned off stays
off, and one it does not mention is turned on.

The bundled originals, classic Add-Ons such as the Stunt Plane and the
Duplicator, are never committed. Bootstrap ends by importing the copies this
machine has (`python tools/addon_bundle.py build --missing-ok`, then
`install`) into `content/addons`; a machine without them runs the game
without those Add-Ons. Rerun those two commands after pulling a change to
`packages/default-addons.json` or to a port. See
[release-builds.md](release-builds.md#bundled-original-add-ons).

## Reruns, stale packs and flags

Every pack the script builds gets a stamp in `content/_regeneration/stamps/`
hashing its inputs: the importer's sources and their local path dependencies,
the stamps of the packs it reads, the pinned decompiler and the v20 file
listing (paths and sizes, ignoring `.ml` lighting caches and `config/`). A rerun
prints a plan and then:

- builds packs that are missing;
- rebuilds packs whose run was interrupted (the stamp is written as unfinished
  first, and completed only when the importer succeeds);
- rebuilds packs whose inputs changed, and everything built from them;
- keeps current packs, and keeps packs that have no stamp (copied from a
  playtest package or an older checkout) untouched;
- moves a `content/packages.json` override (which pins older pack names) aside
  to `packages.json.disabled`, so the client loads the packs built here.

Flags (`regenerate_content.py`; `bootstrap.py` takes the ones marked *):

| Flag | Effect |
|---|---|
| `--v20 DIR` * | the v20 install |
| `--content DIR` * | fill another content directory instead of `<repo>/content` |
| `--plan` | print the plan and stop |
| `--rebuild STEP` * | rebuild one pack even if it is current (repeatable) |
| `--keep-stale` * | keep packs whose inputs changed |
| `--from STEP`, `--only STEP` | narrow the steps |
| `--redecompile` | redo the decompile step |

The workspace converters and `bri-client` are always built together as one
package set (a no-op when nothing changed; a narrower set would change cargo's
feature unification and recompile the client). A stale-input rebuild can be large after a
`bri-convert` change, because most packs depend on it. When an importer's
command line changes, bump its recipe number in `STEP_INPUTS` so existing
stamped packs rebuild.

The geometry pass always rejects two stock files on purpose (a brick fragment
that is not a standalone BLB, and a DTS v18 editor marker). The script accepts
those, reports and skips failures in add-ons v20 does not ship, and stops on any
other stock conversion failure.

## Mission lighting

Map lighting is baked by `map_bundle` from the originals, a port of v20's
own sun lighting pass. It does not read `.ml` lighting caches, so the output
does not depend on which maps someone has played. Accuracy against the engine's
own caches is recorded in content-conversion.md ("Mission lighting bake").

Classic and Unified retain those baked inputs. Dynamic uses a separate
`lighting-parameters.json` sidecar containing lamp positions, colors and radii;
its illumination and shadows come from the current scene. Bootstrap runs
`prepare_lighting` after map generation and before the startup check, including
when the map pack was copied in without regeneration stamps. Valid sidecars
are kept; missing or stale ones are recovered offline from the converted source.
Recovery cannot reconstruct light metadata that the original map never retained.

For an existing map pack, without reimporting the original installation:

```sh
cargo run --release --locked -p bri-render --bin prepare_lighting -- content/map-bundle-017
cargo run --release --locked -p bri-render --bin prepare_lighting -- content/map-bundle-017 --check
```

The sidecar is checked against the bundle identity and travels in the private
CI content archive. Packaged startup validation rejects missing or stale
parameters. Interactive Dynamic rendering announces a live sun/ambient fallback
if the sidecar is unavailable; it never silently reuses baked shadows.

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
