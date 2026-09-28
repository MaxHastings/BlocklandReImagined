# Packages

Status: format landed 2026-09-27 (platform API level 1); the client, the
dedicated server and the join check use it (protocol 31). Code:
`crates/package` (`bri-package`). This closes the shape of door-closer P0 items 2 (package
manifest) and 4 (one id grammar); see
[`docs/audits/platform-door-closers.md`](../audits/platform-door-closers.md).

A package is a directory of content under the content root. The base game is
18 packages; mods are more packages in the same list. Alpha rules apply: no
compatibility or migration code, and the format may still break.

## One id grammar

Every piece of content is named `namespace:kind/name`
(`bri_package::id::ContentId`):

```text
v20:brick/brick1x1data
v20:weapon/gunitem
creeper:creature/creeper
```

- `namespace`: 1-32 of `a-z 0-9 _ -`, starting with a letter.
- `kind`: `a-z 0-9 _ -`.
- `name`: 1-64 of `a-z 0-9 _ - .` with inner `/` allowed.

A package's id is the namespace of everything it declares, so two packages
cannot collide and no package can masquerade as another. The base game's
packages are the one exception: they are all called `v20-<part>` and all
declare into `v20` (`id::content_namespace`). The names `v20 bri base core
engine game vanilla blockland server client local system admin package
packages`, and any package id built on one of them (`v20-weapons`), are
reserved for the game itself (`id::is_reserved`).

Versions are `major.minor.patch`; dependency requirements are `*`, `=1.2.3`,
`>=1.2` or `^1.2` (`id::Version`, `id::Requirement`).

### Converted content

Importers name converted content with `id::native(namespace, kind, source)`
(or `id::Minter`, which also refuses two sources that land on the same id).
It lowercases, turns `\` into `/`, and turns every other character outside
`a-z 0-9 _ - . /` into `_`. Files referenced by path use the kind `file`:

```text
brick1x1Data                              -> v20:brick/brick1x1data
horsearmor::activate                      -> v20:vehicle/horsearmor__activate
Add-Ons/Brick_Large_Cubes/64x Cube.blb    -> v20:file/add-ons/brick_large_cubes/64x_cube.blb
```

### Legacy spellings still in the base packs

The base packs and the code that reads them predate this grammar. A survey
of the 17 client-side base packs (2026-09-27) found 5,617 id occurrences
(1,755 distinct) in three spellings, plus about 400 literals in about 110
`.rs` files:

| Spelling | Used for | Example |
|---|---|---|
| `v20/kind/name` | bricks, prints, sounds, clips, music, emitters, particles, lights, explosions, events, weather | `v20/brick/brick1x1data` |
| `v20.kind.name` | weapons, images, projectiles, shapes, vehicles, players, foliage, weapon debris | `v20.weapon.gunitem` |
| `v20/<path>` | files by virtual path: maps, meshes, textures, interiors | `v20/add-ons/map_slate/slate.mis` |

Names that the grammar cannot hold as-is: spaces (`64x cube.blb`,
`synth 4/synth4_00.wav`), `::` (`v20.vehicle.horsearmor::activate`), a
parenthesised suffix (`rocketexplodesound (alternate definition)`) and `#`
(`slatestormrevised.mis#weather-9`). `id::native` maps each of them. No name
in the survey is longer than 64 characters.

Rename plan, scheduled with the vanilla-as-packages phase and multi-pack
loading, not before: every importer mints through `id::native`; one change
rewrites the runtime literals to `v20:kind/name` (paths to `v20:file/...`)
and points `base-packages.json` at packs regenerated from that importer
commit. Until then, new content (mods, Stress Lab packages) uses the grammar
and the base packs keep their spellings.

## `packages.json`: what a peer loads

The client and the dedicated server both read `packages.json` from their
content root (`bri_package::packages::PackageSet`). Without one they use the
base game's list, `crates/package/base-packages.json`.

```json
{
  "schema_version": 1,
  "packages": [
    { "id": "v20-weapons", "version": "9.0.0", "side": "shared", "dir": "weapons-pack-009", "role": "weapons" },
    { "id": "v20-ui", "version": "3.0.0", "side": "client", "dir": "ui-pack-003", "role": "ui_pack" },
    { "id": "v20-worlds", "version": "5.0.0", "side": "server", "dir": "worlds-pass-005", "role": "worlds" },
    { "id": "creeper", "version": "1.0.0", "side": "shared", "dir": "creeper" }
  ]
}
```

| Field | Rule |
|---|---|
| `id` | Package id (namespace rules above). Unique. |
| `version` | `major.minor.patch`. Base packages use their generation number as the major version (`weapons-pack-009` is `9.0.0`). |
| `side` | `server`: only the server loads it; never compared or sent. `shared`: both simulate with it; must match to join. `client`: presentation only; a difference is reported but does not refuse the join. |
| `dir` | Directory under the content root; plain relative path, no `..`, must stay inside the root. |
| `role` | Optional. The engine system that reads the package directly (`map_bundle`, `brick_catalog`, `geometry`, `effects`, `brick_materials`, `avatar`, `audio`, `weapons`, `item_presentation`, `vehicles`, `events`, `ui_pack`, `effects_runtime`, `weather`, `foliage`, `weapon_debris`, `worlds`, `tutorial`). At most one package per role: it is that kind's *base* package. Packages without a role are still loaded, hashed and agreed on, and add to a kind when they provide it (below). |

Unknown fields are errors. Every problem is a diagnostic with a stable code
(`packages.id`, `packages.duplicate`, `packages.dir`, `packages.role_conflict`,
...), as in `docs/modding/package-format.md`.

Swapping a pack generation, or adding a mod, is a data change to this file,
not a Rust change.

## Environment: what a peer actually loaded

Loading hashes every listed package into an `environment::Environment`: the
platform API level plus, per package, id, version, side, SHA-256 and size.
The hash is over the directory's files (relative path, length and SHA-256 of
each, in sorted order); links are refused.

A joining client sends its `shared` and `client` packages. The server compares
them with its own (`Environment::compare`) and gets one `Mismatch` per
differing package:

```text
server has v20-weapons 9.0.0 (3f2a…), you have v20-weapons 8.0.0 (91c0…)
server has creeper 1.0.0 (aa01…), you do not
you have zombies 2.0.0 (77b2…), the server does not
```

A mismatch in a `shared` package refuses the join and the rejection lists
every difference; `client` differences are told to the joining player in chat
and do not refuse. Server-only packages are never compared.

Where it is used:

- `ClientContent::load` and `ContentPaths` read `packages.json` and resolve
  each engine role to its package directory; `ContentPaths::environment()`
  hashes the set when hosting or joining.
- `bri-server <content-root> <world.json> <state-dir> <listen> [seconds]`
  reads the same file and publishes its environment in `host.json`.
- `Hello.packages` carries the joining client's shared and client packages;
  `ServerOptions.environment` is the server's.
- `tools/regenerate_content.py` builds the packs the base list names, and the
  playtest packager copies the effective list into `content/packages.json`.
- Mod packages with their own `package.json` (the Stress Lab packages) are
  listed here like any other package; `bri-package-runtime` loads their
  manifests from the same list.

## Turning packages on and off

`packages.json` is also the enabled list. A disabled package's exact entry
moves to `packages-disabled.json` in the same content root (same schema), and
package directories holding a `package.json` that neither file lists are
discovered as disabled. `bri_package::library` owns scanning, dependency
planning and atomic rewrites; loaders only read `packages.json`. The in-game
Add-Ons screen is built on it: see [`mod-manager.md`](mod-manager.md).
## Content from several packages

A role names a kind's *base* package, not its only one. Loading builds one
merged pack per kind from the base package plus every role-less package in
`packages.json` whose directory holds that kind's file under `assets/`, in
list order (`bri_net::content_identity::kind_providers`). The dedicated server
(`bri_net::dedicated`, which `bri-server` runs), a client-hosted game and a
joining client all load this way (`ContentPaths::weapon_content`,
`item_physics`, `vehicle_pack`, `brick_extras`).

| Kind (`provides.kind`) | File under `assets/` | Merge |
|---|---|---|
| `weapons` | `weapons.json`; optional `presentation.json` and `item-physics.json` beside it for drawing and drop bounds | `bri_weapons::Pack::merge`, `WeaponContent::load_with`, `ItemPhysicsContent::load_with`, client `ItemAssets::load_with` |
| `vehicles` | `vehicles.json` | `bri_vehicles::schema::Pack::merge`, client `VehicleAssets::load_with` |
| `bricks` | `brick-catalog/stock-catalog.json` with `catalog-audit.json`, `native-collisions.json` and the mesh files beside it (the stock catalog layout) | `Definitions::load_with` |

The package runtime accepts these kinds in a package's `provides`
(`crates/package-runtime/src/content.rs` `Kind`), and `bri-import-addon`
declares them.

Rules:
- Ids are namespaced, so packages cannot collide. A duplicate weapon or
  vehicle id keeps the earlier package's and is reported; a duplicate brick
  id is an error. Explosions and damage types are still keyed by bare Torque
  name, so a clash there is reported too.
- A weapon reference no loaded package satisfies drops only the image or item
  that needs it, with a diagnostic (`merge: ...`, printed by `bri-server`). It
  never refuses the whole set.
- A merged resource or asset records its package directory
  (`Resource::package`, `Asset::package`). Paths stay relative to their own
  package. Base packages sit directly under the content root, and
  `bri_weapons::resource_root` and `bri_vehicles::asset_root` resolve a package
  beside them.
- Without extra packages, loading is byte-for-byte what it was: the same packs
  and the same weapon fingerprint.
- Systems still take one pack each, and the wire carries ids that were already
  strings, so this needs no protocol or save change. The join check already
  requires identical `shared` packages on both sides.

Not merged yet: imported bricks do not appear in the brick selector, because
the selector's icons come from the UI pack. Imported sounds and effects are
not merged either.

## Enabling and disabling packages

Enabled means exactly the entries in the content root's `packages.json`, and
loading reads nothing else. The in-game mod manager
(`crates/package/src/library.rs`) moves disabled entries to a sibling
`packages-disabled.json` with the same `PackageSet` schema, and treats
unlisted directories holding a `package.json` (up to three levels deep) as
discovered and disabled. A package imported by `bri-import-addon` becomes
loadable by adding the `packages.json` line its report prints (`side`
`shared`, no `role`).

## Per-package manifests (`package.json`)

The mod platform lane defines the manifest a mod package carries inside its
directory (`package.json`: id, version, api, license, provenance,
dependencies, capabilities, provides, slots), its archive format and its
download cache, in `docs/modding/package-format.md`. They build on this crate:
the same id grammar, versions, diagnostics and `PackageRef`/`Mismatch` shapes.
Base packages carry no `package.json`; `packages.json` describes them.

## Not built yet

Dependency resolution, downloading missing packages, and archives are the mod
platform lane's. Renaming the base packs' ids follows the plan under
"Legacy spellings" above.
