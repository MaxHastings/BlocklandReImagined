# Package format (platform API level 1)

A package is a folder with a `package.json` manifest. It is the unit a server
enables and a client downloads. Code: `crates/package` (`bri-package`).

```text
my-package/
  package.json          the manifest (client data: clients read it too)
  assets/...            client data: textures, sounds, models, definitions
  server/...            server-only: behaviour scripts; never sent to clients
```

## Manifest

```json
{
  "schema_version": 1,
  "id": "creeper",
  "version": "1.0.0",
  "api": 1,
  "name": "Creeper",
  "description": "A hissing block that explodes near players.",
  "authors": ["Max"],
  "license": "CC0-1.0",
  "provenance": { "source": "original" },
  "dependencies": { "creature-kit": ">=1.2" },
  "capabilities": ["players.read", "players.health", "chat.send"],
  "provides": [
    { "kind": "behaviour", "id": "creeper:behaviour/main", "file": "server/main.luau" },
    { "kind": "asset", "id": "creeper:asset/hiss", "file": "assets/hiss.ogg" }
  ],
  "slots": []
}
```

| Field | Rule |
|---|---|
| `schema_version` | `1`. |
| `id` | The package id **and the namespace of everything it declares**. 1-32 chars, `a-z 0-9 _ -`, starts with a letter. Reserved: `v20 bri base core engine game vanilla blockland server client local system admin package packages`. |
| `version` | `major.minor.patch`. |
| `api` | Platform API level the package needs; this build provides `1` (`bri_package::API_LEVEL`). |
| `license`, `provenance.source` | Required. SPDX id or `proprietary`; `original` or where assets came from. |
| `dependencies` | Package id to requirement: `*`, `=1.2.3`, `>=1.2`, `^1.2` (a bare `1.2` means `^1.2`). |
| `capabilities` | What server behaviour may do. Unknown names are errors. Listed in `crates/package/src/capability.rs`. |
| `provides` | Declared content. Each `id` is `<package id>:<kind>/<name>`, with a kind a system in this build consumes (`crates/package/src/kind.rs`). |
| `slots` | Named slots the package fills, `add` (composes) or `replace` (exclusive). Listed in `crates/package/src/slot.rs`. |

Unknown fields are errors, so typos are caught rather than ignored.

## Files

- Paths: letters, digits, `.`, `_`, `-`; `/` separators; no spaces, `..`,
  `:`, trailing dots or Windows device names; at most 160 characters. Two
  paths differing only in case are refused.
- Client data types: `json txt md csv png jpg jpeg tga dds bmp ogg wav glb
  gltf bin blb bls ttf otf`. Code types (`exe dll js lua luau wasm cs ...`)
  outside `server/` are refused with `file.code_in_client_data`.
- `server/` types: `luau json txt md`.
- Symlinks are refused; dot-files are skipped.
- Limits per side: 2048 files, 32 MiB per file, 128 MiB total.

## Identity and archives

The client side is packed into one deterministic archive (`BRIPKG\0\x01`,
sorted table of path, size and SHA-256, then the bytes). Its SHA-256 is the
package's identity on the wire: two packages with the same bytes have the
same hash on every machine. `server/` is packed separately and its hash is
reported by `check`, but it is never published or sent.

## Diagnostics

Every problem has a stable code, a location (`package.json#/provides/0/id`
or a file path), a message and usually a fix, as JSON
(`bri_package::diag::Diagnostic`).
