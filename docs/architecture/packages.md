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
| `role` | Optional. The engine system that reads the package directly (`map_bundle`, `brick_catalog`, `geometry`, `effects`, `brick_materials`, `avatar`, `audio`, `weapons`, `item_presentation`, `vehicles`, `events`, `ui_pack`, `effects_runtime`, `weather`, `foliage`, `weapon_debris`, `worlds`, `tutorial`). At most one package per role. Packages without a role are still loaded, hashed and agreed on. |

Unknown fields are errors. Every problem is a diagnostic with a stable code
(`packages.id`, `packages.duplicate`, `packages.dir`, `packages.role_conflict`,
...), as in `docs/modding/package-format.md`.

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

## Per-package manifests (`package.json`)

The mod platform lane defines the manifest a mod package carries inside its
directory (`package.json`: id, version, api, license, provenance,
dependencies, capabilities, provides, slots), its archive format and its
download cache, in `docs/modding/package-format.md`. They build on this crate:
the same id grammar, versions, diagnostics and `PackageRef`/`Mismatch` shapes.
Base packages carry no `package.json`; `packages.json` describes them.

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
  joins, and on that refusal, unless the client runs a shared package the
  server lacks, fetches the server's packages, hands them to the caller's
  `load` step and joins again with the list it returns; the server checks
  that list like any other.

## Not built yet

- The game client joins servers through `connect_fetching`: downloaded
  packages go to `<state>/package-cache`, `bri_client::mods::load_fetched`
  loads them (`Catalog::load_dirs`: models, HUD panels and other data) and
  the view carries them as `View::mods`. A server running different base
  game content is refused with that reason, because base content cannot be
  swapped while the game runs. Hosting yourself needs no download.
- `bri-server` passes `packages: None` until it loads mod packages through
  `packages.json`.
- The whole join, downloads included, shares the client's 120 s connect
  timeout; a large download needs its own.
- A host that edits a package must restart to offer the new version; there
  is no reload.
- Per-package `package.json` manifests, dependency resolution and archives.
- Content inside the base packages still uses older id spellings
  (`v20/brick/...`, `v20.weapon....`) until those packs are regenerated under
  the grammar above; see the audit.
