# Porting v20 Add-On scripts

Import Add-On turns an old Blockland Add-On's datablocks into data, but it
never runs or translates its scripts. What the scripts did is listed in the
import report as **needs behaviour**. A **port** is the native rewrite of that
behaviour, checked against v20 and listed so everyone's import gets it.

This page is the recipe, for a person or for the agent they hand it to.

## What a port is

A port lives in [`crates/addon-import/ports`](../../crates/addon-import/ports):

- [`ports.json`](../../crates/addon-import/ports/ports.json) is the list. Each
  entry names one v20 Add-On, the port that covers it and whether it is
  `verified` or `partial`.
- `ports/<port>/port.json` holds the port itself: changes to the files the
  importer writes, as JSON merge patches ([RFC 7396](https://www.rfc-editor.org/rfc/rfc7396)).
- `ports/<port>/files/` holds any files the port adds, at the path they get
  inside the imported Add-On.

A port carries only the new work. The original Add-On's models, sounds and
scripts come from the player's own copy when they import it, so the list can
ship with the game without redistributing anyone's files. The importer is
built with the list, so **Start Game > Add-Ons > Import** applies ports with
nothing to download.

When the importer reads an Add-On whose folder name is listed, it checks the
functions the port covers. If they match, it applies the port. The report's
`ports` section says what it changed, and each covered `needs_behaviour` entry
names the port. If they do not match (a different version of the Add-On), it
changes nothing, and the report names the port and says which part did not
match.

### A list entry

```json
{
  "addon": "Weapon_Shotgun",
  "title": "Sawn-off Shotgun",
  "port": "weapon_shotgun",
  "status": "verified",
  "sha256": [],
  "covers": {
    "shotgunImage::onFire": {
      "projectiles": "%shellcount\\s*=\\s*(\\d+)\\s*;",
      "spread": "%spread\\s*=\\s*([0-9]*\\.?[0-9]+)\\s*;"
    }
  },
  "tests": ["crates/addon-import/tests/ports.rs shotgun_port_fires_the_spread"]
}
```

| Field | Meaning |
|---|---|
| `addon` | The v20 folder or zip name, which is the Add-On's identity. |
| `port` | The folder under `ports/` holding the port. |
| `status` | `verified`: tests show it behaves like v20 for everything the Add-On's scripts do. `partial`: it covers some functions and the rest are still missing. |
| `sha256` | `source.sha256` from the import report of each copy the port was checked against. The report calls a copy `listed` or `unlisted`; both get the port if they match. |
| `covers` | Each function the port replaces, with named patterns (regular expressions, case-insensitive) its body must match. The first group of each is a value the port can use. |
| `tests` | `path test_name` for each test proving the port. The list's own test checks that they exist. |

Patterns do two jobs. They prove the copy is the shape the port was written
for, and they read the numbers from that copy's script, so a port never
hard-codes one copy's values. In a patch, a string that is exactly
`"{projectiles}"` becomes the captured value (a number when it reads as one).
`{name}` inside a longer string becomes its text.

### A port

```json
{
  "schema_version": 1,
  "notes": "What the port does, in a sentence or two.",
  "patch": {
    "assets/weapons.json": {
      "images": {
        "weapon_shotgun:image/shotgunimage": {
          "shot": { "projectiles": "{projectiles}", "spread": "{spread}", "recoil": "{recoil}" }
        }
      }
    }
  }
}
```

Patch keys are files the importer wrote (`assets/weapons.json`,
`assets/vehicles.json`, `package.json`). A patched `weapons.json` must still
pass the weapons pack's checks, or the port is not applied. The patch is all
or nothing: a port that fails anywhere changes no file.

## The recipe

1. **Import the Add-On** from a checkout:

   ```sh
   cargo run -p bri-addon-import --bin bri-import-addon -- Weapon_Example.zip out/weapon_example
   ```

   Add `--reference "<v20 folder>"` when you have one, so the Add-Ons it
   leans on are reported as dependencies.

2. **List the work.** In `out/weapon_example/import-report.json`, each
   `needs_behaviour` entry with no `port` is one function to port. It gives
   you:
   - `source` and `end_line`: the function to read.
   - `hook`: what it attaches to, and `native_default`, what the game does
     today without it.
   - `operations`: the engine calls it makes, each with its line.
   - `missing_capabilities` and `runtime_hook`: what the platform does not
     offer yet.
   - `entity_state` and `blockers`: per-object fields, `eval`, `call` and loops.

3. **Choose the native form** for each function:

   | The function | Port it as |
   |---|---|
   | An image's `onFire` using v20's spread code (`%shellcount`, `%spread`, a `setVelocity` recoil) | the image's `shot` data (below) |
   | A fire-rate check on `%obj.lastFireTime` and `minShotTime` | nothing: the image's `min_shot_ticks` already does it, from the datablock |
   | Anything a field in [Making Add-Ons](README.md) section 5 or 6 expresses | a patch setting that field |
   | A `serverCmd` in an Add-On with no weapons, vehicles or bricks | a rule (section 3 of the guide): `behaviour.json` and a script under `files/`, and a `package.json` patch adding them to `provides` and their `capabilities` |
   | Anything whose `runtime_hook` is null or that needs a missing capability | not portable yet: port the rest, mark the entry `partial`, and say what is missing in `notes` |

   Keep the Add-On's own data where the importer put it. A port changes what
   the scripts changed, nothing else. A rule is host-only and weapons data is
   for everyone, and one Add-On cannot be both, so an Add-On that needs both
   is `partial` for now.

4. **Write the patterns** for every number or name the port relies on. Match
   the script's own spelling loosely (`\s*` around `=`), and capture the
   value, not the whole line.

5. **Write `ports/<port>/port.json`** and any `files/`.

6. **Prove it behaves like v20.** Add a test to
   [`crates/addon-import/tests/ports.rs`](../../crates/addon-import/tests/ports.rs):
   - Write a **stand-in** Add-On under `tests/fixtures/ports/<Addon_Name>`:
     the same folder name and the same function shape, with its own numbers,
     marked CC0. Never check in the original: community Add-Ons carry no
     licence.
   - Import it with the built-in ports and assert the port applied, with the
     values read from the stand-in.
   - Assert the behaviour from the v20 script's own formula, not from the
     port's code. `shotgun_port_fires_the_spread` checks the pellet count,
     that each pellet's speed includes the recoil it inherits, and that each
     pellet turns by at most √3·5π·spread from the aim.
   - Assert what a player would notice in a hosted `Session` when it matters
     (`ported_shotgun_recoils_the_shooter_in_a_hosted_game`).
   - Where the original exists on your machine, extend `real_community_samples`
     in `tests/import.rs` so the real copy is checked too, and add its
     `source.sha256` to the list.

7. **List it** in `ports.json` with its status and tests, then run
   `cargo test -p bri-addon-import`.

## Handing it to an agent

Give your agent this page, the Add-On and this prompt:

> Port the Add-On `<Addon_Name>` to Blockland ReImagined following
> `docs/modding/porting.md`. Import it, read `import-report.json`, and port
> every `needs_behaviour` entry that the recipe's table says is portable.
> Read the numbers from the script with patterns, never hard-code them.
> Write a CC0 stand-in fixture with the same folder name and function
> shapes, test the port against v20's own formulas, list it in
> `crates/addon-import/ports/ports.json` as `verified` or `partial`, and run
> `cargo test -p bri-addon-import`. Do not commit the original Add-On.

## Ports so far

| Add-On | Port | Status | What it covers |
|---|---|---|---|
| `Weapon_Shotgun` (Sawn-off Shotgun) | `weapon_shotgun` | verified | `shotgunImage::onFire`: the pellets, their spread and the recoil, read from the copy's own script |

## The image `shot` field

v20's most common scripted weapon is the spread code in `onFire`: push the
shooter back along their aim, then fire `%shellcount` projectiles, each
turned by random Euler angles of up to ±5π·`%spread` radians about each axis.
The image's `shot` does the same from data:

| Field | Meaning |
|---|---|
| `projectiles` | projectiles per shot, 1 to 64 (`%shellcount`) |
| `spread` | v20's `%spread`, 0 to 1 |
| `recoil` | speed the shooter loses along their aim, in units per second (the `-n` in the recoil line); the projectiles inherit it, as in v20 |

The random angles come from the tick, the shooter and the pellet number, so
the host and every player compute the same spread with nothing sent.
