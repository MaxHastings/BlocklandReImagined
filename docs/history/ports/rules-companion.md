> **Historical note. Written 2026-10-01 (the file's date in the cloud project folder, `ports/rules-companion.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Host rules for imported Add-Ons (the `-rules` companion)

This is for a port that needs a host rule as well as patched weapon data. An
imported Add-On is one `shared` package: its items, images and bricks go to
every player. A shared package cannot carry host code. So the port's rules
become a second Add-On beside the import, which only the host loads, and the
import turns that Add-On on and off with itself.

The code is in `crates/addon-import/src/ports.rs`. The test is
`crates/addon-import/tests/ports.rs`
`port_rules_become_a_host_only_companion_turned_on_with_the_import`.

## 1. Where the rule lives

```
crates/addon-import/ports/<port>/
  port.json          { "schema_version": 1, "notes": "...",
                       "rules": { "capabilities": ["player", "world.edit", "chat"] },
                       "patch": { ... } }
  rules/
    behaviour.json   the rule's behaviour, as in any Add-On (its "script" names the file below)
    <name>.rhai      the script (one or more .rhai files)
  files/             (optional) files added inside the import itself, as before
```

- `rules` in `port.json` turns the companion on. `capabilities` is required
  and non-empty.
- `rules/` may hold only `behaviour.json` and `.rhai` files.

On import the importer writes the companion next to the import's folder:

| | The import | Its rules |
|---|---|---|
| Folder | `addons/<ns>` | `addons/<ns>-rules` |
| Id | `<ns>` (from the Add-On's folder name, e.g. `Tool_FillCan` → `tool_fillcan`) | `<ns>-rules` |
| Side | shared | server (players never get it) |
| package.json | the importer's, plus `"companions": ["<ns>-rules"]` | written by the importer: name "<title> (host rules)", CC0, your `capabilities`, `dependencies: { "<ns>": "=<version>" }`, provides `behaviour` + `script` |

- Turning the import on in the Add-Ons screen also turns on its rules, after
  it. Turning it off turns them off.
- The importer checks the rules exactly as the game loads them: the
  manifest, `behaviour.json` and that its script exists. If anything fails,
  the whole port is not applied (all or nothing).
- `import-report.json` has `ports[].rules` with `{ id, dir,
  packages_json_entry, files }`.

## 2. How the rule names the import's ids

Rules files are text. `{{name}}` anywhere in them is filled in at import:

| Placeholder | Value |
|---|---|
| `{{namespace}}` | the import's id/namespace, e.g. `tool_fillcan` |
| `{{rules}}` | the rules' own id, `<ns>-rules` |
| `{{version}}` | the import's version |
| `{{<pattern>}}` | any value a `covers` pattern in `ports.json` captured from this copy's script |

A `{{word}}` that names no value is an error, so typos are caught.

The importer's ids are `<ns>:<kind>/<datablock name, lower case>`, for
example `{{namespace}}:weapon/fillcanitem` and
`{{namespace}}:image/fillcanimage`. Read the exact ids from the import's
`assets/weapons.json`.

In a rhai script, for example:

```rhai
fn tool() { "{{namespace}}:weapon/fillcanitem" }
fn limit() { {{limit}} }            // captured by a covers pattern
```

## 3. How the image reaches the rule's command

In the merge patch, `{namespace}`, `{rules}` and `{version}` are available
too, written with single braces as captured values are. They are filled in
keys as well as values. Point the imported image at the rule's commands
(`crates/weapons/src/lib.rs`, `Image.command` and `Image.commands`):

```json
"patch": {
  "assets/weapons.json": {
    "images": {
      "{namespace}:image/fillcanimage": {
        "command": "{rules}:fill",
        "commands": {
          "states": { "oncharge": "{rules}:charge", "onabortcharge": "{rules}:release" },
          "jet": "{rules}:alt",
          "light": "{rules}:reload",
          "wheel": "{rules}:wheel",
          "cancel": "{rules}:cancel"
        },
        "paint_tint": true
      }
    }
  }
}
```

- `command`: the image's `onFire`. It runs the rule's `cmd_<name>(player)`
  instead of firing a projectile, aimed where the holder looks.
  `behaviour.json` sets the command's `aim_reach` and `cooldown_ticks`.
- `commands.states`: entering a state whose `stateScript` is that name (lower
  case: `oncharge`, `onabortcharge`, `onfire`, any other) runs the command.
  Fire, charge and release (a swing) are the image's own states, so they
  map here.
- `commands.jet`, `light`, `wheel` and `cancel` are the other keys while the
  image is in hand. `wheel` passes the notches as an `int` argument.
- After patching `weapons.json` the importer re-pins `presentation.json`, as
  before.

The rule's `behaviour.json` declares each command by the name after the colon:

```json
{ "schema_version": 1, "script": "fill.rhai",
  "commands": [ { "name": "fill", "aim_reach": 32, "cooldown_ticks": 30 } ] }
```

## Status

The branch and commit carrying this are sent through the coordinator. The
layout is also in `docs/modding/porting.md` ("Host rules") on that branch.
