# Packages

Authors making Add-Ons start with the guide in
[docs/modding/README.md](../modding/README.md) and the samples in
`packages/samples/`; this page is the engine-side format.

Status: format landed 2026-09-27 (platform API level 1); the client, the
dedicated server and the join check use it. Code:
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
base game's list, `crates/package/base-packages.json`, followed by the default
Add-Ons installed under the root (`packages/default-addons.json`, held at
`addons/<id>`; `bri_package::defaults`). That is the list a release ships in
its own `packages.json`; a source checkout's content has none, so it keeps
following the base list as it changes.

```json
{
  "schema_version": 1,
  "packages": [
    { "id": "v20-weapons", "version": "9.0.0", "side": "shared", "dir": "weapons-pack-009", "role": "weapons" },
    { "id": "v20-ui", "version": "4.0.0", "side": "client", "dir": "ui-pack-004", "role": "ui_pack" },
    { "id": "v20-worlds", "version": "6.0.0", "side": "server", "dir": "worlds-pass-006", "role": "worlds" },
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
...), defined in `crates/package/src/packages.rs`.

**One path rule.** Every name a package uses for a file or directory, a
`dir` here, a `provides` file in `package.json`, or a path in a download
listing, passes the same check (`bri_package::path::problem`): forward
slashes, no empty, `.` or `..` parts, no character Windows forbids or gives a
meaning (`:` would name an alternate data stream), no part ending in a space
or dot, no device names, at most 160 bytes. Files are then opened through
`bri_package::path::inside`, which refuses any link or junction on the way,
so a package reads only its own bytes. Readers do not keep their own copies
of this rule.

**Conflicts are reported, never last-wins.** A content id provided twice in
one manifest is `manifest.provide.duplicate`; two HUD panels that bind one
key to different commands are `set.hud.key.conflict`.

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

A mismatch in a `shared` package refuses the first join attempt and the
rejection lists every difference; the client then fetches the server's copies
and joins again with `accept_differences`, which the server lets in, telling
the player whatever still differs. `client` differences are told to the
joining player in chat and do not refuse. Server-only packages are never
compared.

Where it is used:

- `ClientContent::load` and `ContentPaths` read `packages.json` and resolve
  each engine role to its package directory; `ContentPaths::environment()`
  hashes the set when hosting or joining.
- `bri-server <content-root> <world.json | resume> <state-dir> <listen>
  [seconds]` reads the same file, publishes its environment in `host.json`
  and offers its packages for download.
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
- How an Add-On looks never stops a load, a host or a join
  (`crate::cosmetic` in the client). Every item, image and projectile of a
  merged weapons pack is presented: from the Add-On's own
  `presentation.json` when it has one that matches its `weapons.json`, else
  from the stock models and icons it names, else with no model and, in the
  item HUD, the item's first letter (v20's `handleItemPickup` fallback). The
  item HUD is built from the weapon list, so the two cannot disagree. A
  missing or broken Add-On texture, model, explosion shape, vehicle model,
  death icon or brick icon gets a stand-in and a console line naming the
  Add-On (`bri_package::library::add_on_label`) and the file. Gameplay data
  that cannot work (an unreadable `weapons.json` or `vehicles.json`, drop
  bounds that do not match their weapons) still stops the load, and the
  message the player sees names the Add-On and the file. `bri-addon-check`
  warns (`check.weapons.presentation`) when the presentation is missing or
  made for a different `weapons.json`.
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

Imported bricks join the brick menu under the category and subcategory they
declare, with icons from the package's `brick-catalog/brick-icons.json`
(`install_package_bricks` in `crates/client/src/content.rs`). A brick
without a stored icon shows none. Imported sounds and effects are not merged
yet.

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

A mod package carries a `package.json` in its directory: `schema_version`,
`id`, `version`, `api`, `name`, `description`, `authors`, `license`,
`provenance`, `dependencies`, `capabilities` and `provides`. It is parsed by
`bri_package_runtime::manifest` with the id grammar, versions and
diagnostics above. Unknown fields are errors, so a misspelt field is
reported rather than ignored. Base packages carry no `package.json`;
`packages.json` describes them. The author-facing walkthrough is
[docs/modding/README.md](../modding/README.md).

- **Capabilities** are listed once, in `bri_package::capability`, each with
  the plain words players read ("send chat messages"). The runtime refuses any
  operation whose capability the manifest lacks (`ops::authorize`).
- **Sides follow content kinds.** Behaviour, script, world and entity are
  server content; model and HUD are client content. A package holding only
  server kinds is `server`, only client kinds `client`, none `shared`. A
  package mixing both cannot load on either side and must be split. Client
  code makes a package `shared` (the host decides and joiners download it)
  unless its `client` section says `"personal": true`, which keeps it
  `client`; code cannot ride on a `server` package
  (`bri_package::library::side_for_package`). The manifest decides: an
  Add-On's `side` in `packages.json` follows it when the list loads.
- **`bri-addon-check <folder> [--json]`** (`bri_package_runtime::check`)
  loads one Add-On the way the game does, with the Add-Ons it needs found
  beside it. It checks the manifest, files, HUD bindings and scripts, and
  prints the side, what the Add-On provides, what it may do and what it
  needs. It runs nothing.

The modplatform draft (PR #6) had a luau behaviour host, a single-archive
package identity, a content-addressed store and exclusive `slots`. Main
chose Rhai behaviour, directory hashing (`environment::hash_dir`) and the
file-level download cache of the package sync work, so those parts were not
carried over. Only its plain-language capabilities, strict manifests and the
`check` report were kept. An exclusive slot for the server's game mode
returns with the game mode picker, which is where it is first read.

## Client code (`client` in `package.json`)

An Add-On may carry code that runs on players' machines: a WebAssembly
module and WGSL shaders, sandboxed and presentation only. It is declared in
the `client` section of the Add-On's own `package.json`:

```json
"client": {
  "module": "client/main.wasm",
  "capabilities": ["render.layer", "render.shader"],
  "shaders": ["client/cube.wgsl"],
  "sounds": [],
  "personal": false
}
```

The host decides whether it runs: a server running the Add-On sends it to
every joiner, and a server that does not leaves joiners' own copies off.
`"personal": true` makes it each player's own choice for their own screen
instead, run on every server they join and never sent.

Capabilities have tiers: `render.layer`, `render.shader`, `audio`,
`input.focused`, `net.message` and `world.read` are sandboxed (the player trusts the
server once); `net.http` and `files.addon_folder` are elevated (a separate,
stronger per-Add-On choice); `native` (a native plugin) is elevated too,
needs the server's name typed on the prompt, and does not run yet. A package with client code travels like any other
`shared` package; the trust prompt comes before its code runs, and is
skipped when the player installed the same code themselves. Checks, host API, budgets and prompts:
[client-sandbox.md](client-sandbox.md). Code: `crates/client-sandbox`.

## Distribution: clients fetch what they lack

Code: `bri_package::sync` (listings, cache) and `bri_net::packages`
(transport). Tests: `cargo test -p bri-package sync` and
`cargo test -p bri-net --test package_sync`.

- **Listing.** A server lists each package it offers: every file's relative
  path, size and SHA-256, in path order. The entries hash to the package hash
  by the same rule as `hash_dir`, so a client checks a listing against the
  hash the server's environment promised before fetching anything.
- **What is offered.** Only `shared` and `client` packages of the server's
  environment (`PackageShelf`). `server` packages and any file outside an
  offered package cannot be requested: objects are served by hash, and only
  hashes of offered files resolve.
- **Data, never code.** A listing is refused, on both sides, if any path is
  unsafe to create on Windows (absolute, `..`, `\`, `:`, device names like
  `nul` or `com1`, trailing dot or space, names that differ only by case, a
  file that is also a directory) or if any file type is code Windows could
  run (`CODE_EXTENSIONS`: `.exe .dll .bat .ps1 .js .lnk ...`). A server cannot
  even build a shelf containing one.
- **Budgets.** 256 MiB per file, 2 GiB per package, 65,536 files per
  package, and 4 GiB per fetch on the client (`MAX_FETCH_BYTES`), so a
  hostile server cannot fill a disk with valid packages. Download connections
  are bounded to 16 in total and 2 per address, and close after 15 s idle.
- **Cache.** `objects/<sha256>` holds each file once, so an asset two packages
  share downloads once. A file becomes visible only after its size and hash
  check out; an interrupted or corrupt download leaves nothing behind, and a
  retry fetches only what is still missing. A package is installed by copying
  its objects into a staging directory, re-hashing it with `hash_dir`, and
  renaming it to `packages/<package hash>`, so an installed directory is
  always complete and is exactly the package the server loaded.
- **Trust follows the bytes in use.** Nothing is trusted because it exists
  or was checked earlier. An installed package has a seal
  (`packages/<hash>.seal`: each file's size and modification time); when the
  directory no longer matches it, it is re-hashed and removed if it is no
  longer the package. Objects are checked by size before reuse and by hash as
  each is copied into a package; a damaged one is deleted so the next fetch
  replaces it. Every writer stages under its own unique name, so concurrent
  fetches into one cache are safe. On the server, each offered file keeps
  the size and modification time it was listed with, and a file the host
  changed since is refused, naming the package.
- **Bounded.** After every fetch the cache is pruned to 8 GiB
  (`CACHE_BYTES`): least recently used packages go first, the fetched
  server's packages always stay, objects are removed once settled (every
  installed package holds its own files), and anything touched in the last
  hour may belong to a fetch in progress and stays.
- **Protocol.** A download is its own connection: `JoinBegin { purpose:
  Download }`, then `DownloadRequest::{Environment, Listing, Object}` answered
  in order (object ranges up to 1 MiB). No identity or game state is involved.
  `bri_net::packages::fetch_missing` does the whole fetch and reports bytes
  under `Stage::DownloadingPackages`.
- **Join.** A join whose shared packages differ is refused with
  `Message::PackagesDiffer` (the mismatches, typed), which a client sees as
  the error `bri_net::client::PackagesDiffer`. `Client::connect_fetching`
  joins, and on that refusal fetches every package the server offers that
  the client lacks or has in another version (shared packages only the
  client runs are left out), hands them to the caller's `load` step and
  joins again with the list it returns and `accept_differences` set. The
  server lets that join in with whatever still differs and tells the player
  what is missing, so a join never fails over Add-Ons.
  The refusal's text is `environment::refusal`, which the Add-Ons screen
  reads back into rows; the download uses the join's `HostPin`, so it
  reaches the same host the join trusts.
- **Client and server.** The game client joins through `connect_fetching`:
  downloaded packages go to the content root's `.downloads` cache and
  `bri_client::mods::load_fetched` loads them. A server running different
  base game content is refused with that reason, because base content cannot
  be swapped while the game runs. `bri-server` offers the packages its
  `packages.json` lists.

## Not built yet

- A host that edits a package must restart to offer the new version; there
  is no reload.
- Dependency resolution and archives are the mod platform lane's. Renaming
  the base packs' ids follows the plan under "Legacy spellings" above.
- Content inside the base packages still uses older id spellings
  (`v20/brick/...`, `v20.weapon....`) until those packs are regenerated
  under the grammar above; see the audit.
