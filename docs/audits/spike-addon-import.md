# Spike (c): importing messy community Add-Ons

Date: 2026-09-27. Spike (c) from
[`platform-principles.md`](../architecture/platform-principles.md): take real
community Add-Ons through one command, see what converts, and write down every
seam the platform needs. Package format and id grammar:
[`packages.md`](../architecture/packages.md).

**Result.** `bri-import-addon` converts a community Add-On zip or folder into a
package directory with `package.json`, native data under `assets/` and a
report (`import-report.json` plus `IMPORT-REPORT.md`). It imported all three
samples end to end. The imported shotgun fires in the weapons runtime and the
imported Blocko Car drives in the vehicles runtime, both headless. Neither can
load in a hosted game yet, because every engine system still reads exactly one
pack per role (seam 1). No TorqueScript is executed, and there is no VM.

## Recommendations, in order

1. **Let every system read its content kind from every loaded package** (seam
   1), with cross-package id references (seam 3) and runtime kinds for the
   imported content (seam 2). Until this lands, no imported Add-On can load in
   a hosted game, whatever else is fixed.
2. Give image state scripts, and the other callbacks Add-Ons hook, a declared
   behaviour hook in the package runtime (seams 5 and 7). Add the capabilities
   the archive needs most: `schedule`, `entities.animate`, `random.seeded`,
   `players.inventory` and `projectiles.spawn` (seam 6).
3. Add a controllable-entity or bot kind that takes data-driven AI settings
   (seam 8).
4. Extract the effects, debris and audio lowering into libraries, and move
   the importers onto `bri_convert::tscript` (seams 14 and 15).

## The command

```text
bri-import-addon ADDON(.zip|folder) FRESH_OUTPUT_DIR
    [--reference V20_ROOT] [--core RECOVERED_SCRIPT.cs]... [--version 1.0.0] [--json]
```

- `--reference` is a read-only v20 install. Its Add-Ons and `base/` files
  resolve references, so they are reported as dependencies rather than unknowns.
- `--core` adds recovered base scripts (`allGameScripts.cs`, `DamageTypes.cs`),
  which supply base datablocks (`weaponSwitchSound`, `PlayerStandardArmor`) and
  damage types.
- Output: `package.json`, which parses as the package runtime's manifest
  (`bri_package_runtime::manifest`) and loads through `Package::load`. Its
  `license` is an SPDX id found in a licence file, otherwise `proprietary` with
  a provenance note. The directory also holds `assets/weapons.json`,
  `vehicles.json` and `bricks.json` (the existing native pack formats),
  `assets/content.json` (every imported id, kind and file), converted shapes and
  bricks, textures, sounds and the report. The report prints the
  `packages.json` line that would load the package.

Pipeline, following the importer split:

```text
Torque understanding       bri_convert::tscript (new): datablocks, functions, packages,
                           calls with lines; DTS and BLB readers
        ↓
native semantics           bri_weapons_import::lower, bri_vehicles_import::lower (now a
                           library), bri_convert::catalog::read_at, in the Add-On's namespace
        ↓
package + report           crates/addon-import (bri-addon-import)
```

## Report contents

`import-report.json` (schema 1, `crates/addon-import/src/report.rs`):

| Section | What it holds |
|---|---|
| `source` | Name, zip hash, title, authors, description, RTB listing, licence status and files. |
| `package` | Package id and namespace, version, the `packages.json` entry, files written. |
| `assets` | Every file: kind, sha256, status (`converted`, `copied`, `consumed`, `failed`, `unsupported`), output and id. |
| `datablocks` | Every datablock: class, parent, `file:line`, native concept, status (`converted`, `converted_with_gaps`, `consumed`, `recognised_only`, `unsupported`), ids, notes. |
| `ids` | Every id minted, `namespace:kind/name`, with its source. |
| `dependencies` | `ForceRequiredAddOn`/`LoadRequiredAddOn` calls and implicit references, whether the reference has them, the base package and what is used. |
| `unsupported` | Things with no native target: load-time calls and writes, client scripts, `.dso`, bot AI settings, special brick fields. |
| `ambiguous` | References that do not resolve, parents nobody declares, name clashes with vanilla, conventions the importer had to choose. |
| `needs_behaviour` | One entry per script function data cannot express (below). |

A `needs_behaviour` entry targets the sandboxed package runtime that landed on
main during this spike (`crates/package-runtime`: `behaviour` and `script`
kinds, Rhai, capabilities `world.edit`, `damage`, `entity`, `chat`):

- `id`: the suggested behaviour id (`weapon_shotgun:behaviour/shotgunimage.onfire`).
- `function`, `source` and `end_line`: the function to read.
- `hook`: what it attaches to (`image_state_script`, `datablock_callback`,
  `framework_callback`, `global_override`, `console_command`, `helper`) and
  what the native runtime does without it. `runtime_hook` is the runtime hook
  a rewrite can use today (a behaviour command for `serverCmd*`, entity
  `think` for bot callbacks), or null when no such hook exists yet.
- `operations`: engine calls grouped as native operations (`spawn_projectile`,
  `set_velocity`, `set_appearance`, ...), each with `callee@line`.
- `capabilities`: what the host already grants. `missing_capabilities`: what it
  does not.
- `unknown_calls`: calls into a dependency's framework.
- `entity_state`: per-object script fields such as `%obj.lastFireTime`.
- `blockers`: `eval`, `call` and loops.

## Samples

None of them carries a licence, so they are not checked in. The tests use a
synthetic Add-On written for this spike
(`crates/addon-import/tests/fixtures/Weapon_Synthetic_Blaster`, CC0). Real
samples run in `real_community_samples` only where the archive exists.

| Add-On | Shape | Author (description.txt) | Origin | Licence |
|---|---|---|---|---|
| `Weapon_Shotgun` "Sawn-off Shotgun" | weapon with a custom `onFire`: 3 pellets, spread, recoil, fire-rate check; hard dependency on Weapon_Gun | Ephialtes | Maxwell's archive `Documents/_Blockland_Maxwell_1588_Archive/Addons`, RTB id 13 | none stated |
| `Vehicle_Blocko_Car` "Blocko Car" | wheeled vehicle; inherits explosions from base, effects from Vehicle_Jeep | Kaje | same archive, RTB id 516 | none stated |
| `Bot_Zombie` | bot; brick with bot-hole fields, global `Armor::onCollision` override, `eval`, needs the Bot_Hole framework | Rotondo (description says "Hole Mod") | same archive | none stated |

What each import produced (with `--reference` and both `--core` scripts):

| | Shotgun | Blocko Car | Zombie |
|---|---|---|---|
| Datablocks converted / recognised only | 4 / 3 | 7 / 2 | 1 / 1 |
| Assets converted or copied | 11 of 16 | 15 of 24 | 1 of 5 |
| Ids | 16 | 20 | 2 |
| Dependencies (missing) | 2 (0) | 2 (0) | 2 (1: Bot_Hole) |
| Needs behaviour | 1 | 0 | 4 |
| Loads headless | fires one pellet | drives forward | no bot runtime |

### Bulk run over the whole archive

Every zip in the archive was run the same way (255 zips; the archive's other
entries are unpacked map folders). 253 imported. Two failed with a clear error:
`Support_Updater.zip` is not an archive at all (it starts with a line break),
and `System_BlocklandGlass` exceeds the 4096-member budget. No importer panicked.

| Verdict | Add-Ons |
|---|---|
| `converted` (nothing left over) | 31 |
| `converted_with_gaps` | 81 |
| `recognised_only` (no convertible datablocks: client scripts, maps, GUIs, event and support scripts) | 141 |

- Datablocks: 785 converted, 585 converted with gaps, 9 consumed, 419
  recognised only (particles, emitters, debris, player types), 145 unsupported.
  The unsupported ones are mostly `TriggerData` (43), `fxLightData` (33),
  `StaticShapeData` (14) and weapons whose image or projectile lives in a
  missing dependency.
- Assets: 3,036 textures and 281 sounds copied; 737 DTS shapes and 615 BLB
  bricks converted. 21 DTS files failed (15 with a `0xFFFFFFFF` count, 4 with
  an unsupported mesh kind) and 18 BLB files failed (13 with a fractional size
  such as `0.2 0.2 0.8`). Also unsupported: 76 `.dif` interiors, 69 `.gui`
  files, 29 `.bls` saves and 29 `.mis` missions.
- 4,836 functions in 109 Add-Ons need behaviour: 2,697 helpers (mostly client
  GUI code), 833 global overrides, 651 datablock callbacks, 401 image state
  scripts, 246 `serverCmd` console commands and 8 bot framework callbacks.
  Blockers: `eval` or `call` 174 times, loops 1,099 times.
- Missing dependencies seen most often: `JVS_Content` (7), `Weapon_Package_Tier1`
  (7), `Gamemode_Slayer` (5), `Bot_Hole` (4). Only `System_ReturnToBlockland`
  ships a licence file.

The first bulk run found three more vanilla assumptions, fixed here before
the second run:

- 171 bricks inherit from base bricks such as `brick2x2DiscData`, which the
  catalog reader refused as "Unknown parent". Now `read_with_parents` resolves
  them.
- 14 vehicles and some weapons keep scripts and models in subfolders. `./`
  was resolved against the Add-On root instead of the declaring script's
  folder. Now file fields resolve per script.
- `Map_BiomeRacing.zip` is a RAR archive. It is now read through the existing
  7z fallback.


## Seams

Each seam lists the evidence, what it blocks and the fix. "Fixed here" means
this spike changed it.

### Loading imported packages

1. **One pack per role (P0, Phase 2).** Weapons, vehicles, bricks, audio and
   effects each read the single package holding their `role` in
   `packages.json`. The docs say "at most one package per role"
   (`docs/architecture/packages.md`). The imported `weapons.json` is valid and
   loads into `WeaponsWorld` in a test, but a hosted game has nowhere to put a
   second weapons pack. Fix: systems consume a content *kind* from every
   loaded package and merge definitions by id. The role becomes "the base
   package for this kind", not "the only one".
2. **Package kinds (P0, with 1).** The package runtime consumes `behaviour`,
   `script`, `world`, `entity`, `model` and `hud`
   (`crates/package-runtime/src/content.rs` `Kind`), and its manifest refuses
   any other `provides` kind. The importer's kinds (`weapon`, `image`,
   `projectile`, `explosion`, `damage_type`, `vehicle`, `brick`,
   `brick_geometry`, `sound`, `asset`) therefore go in `assets/content.json`,
   and `package.json` provides nothing yet. Each kind needs a consumer and a
   `Kind` entry. Separately, a package's `side` is all-or-nothing, so an
   Add-On with shared data and server behaviour becomes two packages.
3. **References across packages (P0, with 1).** An image's projectile must be
   in the same weapons pack (`Pack::validate`, "Missing projectile"), so an
   Add-On using Weapon_Gun's projectile would get a copy of it under
   `v20:projectile/...`. The importer does this and reports it as ambiguous.
   Explosions and damage types are keyed by bare name in the pack, so two
   Add-Ons both declaring `Shotgun` collide. Vehicle `initial_explosion` and
   `final_explosion` name weapons-pack projectiles. Fix: every cross-reference
   is a `namespace:kind/name` id, resolved across all loaded packages at load
   time. A missing target becomes a diagnostic, not a refusal.
4. **Id grammar inside packs (P0, door-closers owns it).** Weapons and vehicles
   packs still spell ids `v20.kind.name`. Until door-closers lands the
   namespace parameter, the importer lowers with vanilla ids and re-keys this
   Add-On's content to `namespace:kind/name`, and dependencies to
   `v20:kind/name`. `bri_vehicles::Pack::validate` refused any id that did not
   start with `v20.vehicle.`. **Fixed here**: it also accepts a namespaced
   `...:vehicle/...` id.

### Behaviour: what data cannot say

5. **Image state scripts have one fixed meaning (P1, the declared-behaviour
   pilot).** Without a script, `onFire` spawns the image's projectile once
   from data (`crates/weapons/src/runtime.rs` `callback`). The shotgun's burst,
   the recoil and its own fire-rate check live in `shotgunImage::onFire`, so
   today it imports as a one-pellet gun. Fix: the door-closer pilot's
   `behavior` field on `Image`, where a package can name its own behaviour id.
   The needed primitives are: spawn N projectiles with given velocities, read
   the aim and muzzle, apply an impulse to the shooter, and seeded random.
6. **Capabilities the runtime does not have (P1, mod lane).** The runtime
   grants `world.edit`, `damage`, `entity` and `chat`
   (`crates/package-runtime/src/ops.rs`). The samples also need
   `projectiles.spawn`, `players.move` (recoil), `players.read` (aim, position),
   `entities.animate` (playThread), `entities.appearance` (node colours, body
   parts), `entities.kind` (setDataBlock), `players.inventory` (setWeapon),
   `random.seeded` and `schedule`. The runtime's `entity` capability covers only
   the package's own entities, but Add-Ons act on players and on other Add-Ons'
   bots. Across the archive (first bulk run, same operation table), the Add-Ons
   needing each were: `schedule` 60, `entities.animate` 52, `random.seeded` 42,
   `players.inventory` 39, `projectiles.spawn` 30, `entities.appearance` 25,
   `entities.kind` 22 and `effects.spawn` 15.
7. **Global overrides (P2, slots and hooks).** `package` plus `Parent::` lets an
   Add-On replace an engine callback for every object. Bot_Zombie overrides
   `Armor::onCollision` for all players. The native form is a named hook
   (player collision) that behaviours compose on (principle 5), reported as
   `global_override`.
8. **Framework Add-Ons (P2).** Bot_Zombie is configuration plus callbacks for
   Bot_Hole's AI: 40 `h*` datablock fields, and `onBotLoop` and `onBotFollow`
   callbacks. There is no native bot kind to hand those to. Bots are Rust
   brains that join as players (door-closer 9 and the Creeper scenario). A
   package needs a controllable-entity kind with data-driven AI settings, plus
   behaviour callbacks. The report lists the `h*` fields and the callbacks.
9. **Load-time mutation of another package (P1).** The `ForceRequiredAddOn`
   idiom hides the dependency by writing `GunItem.uiName = ""` or
   `JeepVehicle.uiName = ""` at load. A package cannot edit another package's
   content. The intent, "load this dependency but don't list it", belongs on
   the dependency declaration (`hidden` or `listed: false`).
10. **Load-time code, `eval`, `schedule` (by design).** Top-level calls,
    globals and `eval` are reported, never run. `eval` is a blocker for
    mechanical translation.

### Importers that assumed vanilla

11. **Vehicle importer (fixed here).** `bri-vehicles-import` was one `main` with
    a fixed Add-On list, a datablock-to-family table, and ids formatted as
    `v20.vehicle.*` and `v20.projectile.*`. **Fixed here**: the per-vehicle
    lowering is `bri_vehicles_import::lower(name, family, blocks, models, files,
    id)`, and the vanilla binary calls it with the old ids. Vanilla output was
    byte-identical before and after (`diff -r` of a fresh
    `vehicles.json`/models/textures run against `maps-pass-007`). Still vanilla-only:
    - Its datablock regex drops `: parent`, so vanilla vehicles never inherit.
      The add-on path merges inheritance before lowering.
    - Tank and Cannon specifics are matched by datablock name.
    - Wheel steering and power follow the Jeep by index (front two steer, the
      rest drive). Torque sets both from script (`setWheelSteering` and
      `setWheelPowered` in `onAdd`). The Blocko Car has no such script, so the
      report flags the convention as ambiguous.
    - Vanilla models come pre-converted from the map-bundle pass manifest. The
      add-on path converts DTS itself.
12. **Weapon lowering.** `bri_weapons_import::lower` works on any Add-On's
    definitions, but it hardcodes the pack id `v20.weapons.001`, ids through
    `bri_weapons::native_id`, and a Push Broom colour override matched by name.
    Door-closers is adding the namespace parameter. `convert()` walks exactly
    the install's `Add-Ons` folder with a prefix filter. One out-of-range state
    timeout makes `Pack::validate` refuse the whole pack ("Invalid state
    duration"), which happened to 3 archive Add-Ons. It should drop that image
    with a diagnostic, the way `lower` already drops an image whose projectile
    is missing.
13. **Brick catalog reader.** `catalog::read_at` refuses a whole script over one
    dynamic field or unbalanced brace, and stamps `v20/brick/...` (door-closers
    owns the namespace). Special fields (`isBotHole`, `holeBot`, `isDoor`) land
    in `other_properties`. The importer reports them as behaviour.
14. **Effects, debris and audio are fixed vanilla pipelines.** `bri-fx-import`,
    the weapon-debris importer and `bri-audio-import` run from vanilla
    manifests (`tools/regenerate_content.py`). Add-On particles, emitters and
    debris are only recognised. Sounds are packaged, but nothing can play them
    by id. Fix: extract their lowering into libraries the way the vehicles
    lowering was extracted here (P1, when the Cleanup lane next touches them).
15. **Four datablock readers (P1).** `bri-weapons-import` (regex), `bri-vehicles-import`
    (regex, no inheritance), `catalog` (lexer, bricks only) and
    `bri-audio-import` (`tscript.rs`) each parse datablocks. `bri_convert::tscript`
    is the new shared reader: it covers datablocks with parents, functions,
    packages, calls, object creation and globals, all with lines. Migrate each
    importer to it when that importer is next regenerated. Migrating now would
    change the paused weapons owner's output.
16. **Base datablocks exist only in decompiled research files (P1).**
    Dependency resolution against base datablocks and damage types needs
    `--core` pointing at `.research/v20-dso` output, which is never committed.
    Fix: regeneration emits a small committed index (base datablock name to
    class and package) that importers read instead.

### Identity, provenance and safety

17. **Licences.** None of the three samples carries a licence. Across the
    253 imported archive zips, only `System_ReturnToBlockland` does. The manifest accepts an SPDX id
    or `proprietary`. The importer writes an SPDX id it finds in a licence file,
    otherwise `proprietary` plus a provenance note that rights are unknown. The format should say plainly how "unknown" is
    expressed (SPDX `NOASSERTION`), so a server owner can filter on it.
18. **Namespaces.** An Add-On's folder name becomes its package id
    (`Weapon_Shotgun` becomes `weapon_shotgun`). A reserved or invalid name gets
    an `addon_` prefix. Torque's datablock names are global, and the later load
    wins. The report flags an Add-On datablock that shadows a vanilla one.
    `.bls` saves reference UI names, so each imported package needs a
    UI-name-to-id alias table (door-closer 4 already notes this).
19. **Client code and `.dso`.** Client scripts are reported unsupported
    (principle 10). Compiled `.dso` cannot be read.
20. **Bounds.** Members, sizes, path escapes, symlinks and case-only duplicates
    are bounded or refused (`crates/addon-import/src/source.rs`). Output must
    be a fresh directory outside the source and the reference install.

## What an agent does next with a report

For the shotgun: implement `weapon_shotgun:behaviour/shotgunimage.onfire`
against the image's `onFire` hook. The report lists its operations with lines
(`getMuzzleVector@236`, `new Projectile@247`, `setVelocity@227`), the per-object
state (`%obj.lastFireTime`), the missing capabilities
(`projectiles.spawn`, `random.seeded`, `entities.animate`) and the native
default it replaces. Everything else in the package is already data.

## Tests

`cargo test -p bri-addon-import`:

- `synthetic_addon_imports_with_report` imports the CC0 fixture without a
  reference. It asserts ids, statuses, the missing Weapon_Gun dependency with
  its line, both behaviour hooks and their capabilities, the unsupported and
  ambiguous findings, the manifest, and one bolt fired from the imported pack
  in `WeaponsWorld`.
- `refuses_to_overwrite_or_write_inside_the_source`.
- `real_community_samples` imports the three samples against the v20
  reference, fires the shotgun, drives the Blocko Car in `VehiclesWorld` on a
  floor, and checks the zombie's missing Bot_Hole, hooks and `eval` blocker.
  It skips with a message when the archive or reference is absent, so it
  proves nothing on CI.
- `bri_convert::tscript` unit tests cover structure, lines and degradation.
