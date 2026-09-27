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

## Not built yet

Dependency resolution, downloading missing packages, and archives are the mod
platform lane's. Content inside the base packages still uses older id
spellings (`v20/brick/...`, `v20.weapon....`) until those packs are
regenerated under the grammar above; see the audit.
