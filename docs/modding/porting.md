# Porting v20 Add-On scripts

Import Add-On turns an old Blockland Add-On's datablocks into data, but it
never runs or translates its scripts. What the scripts did is listed in the
import report as **needs behaviour**. A **port** is the native rewrite of that
behaviour, checked against v20 and listed so everyone's import gets it.

This page is the recipe, for a person or for the agent they hand it to.

## Port an Add-On in two commands

`bri-import-addon.exe` ships in the game's folder, so none of this needs a
checkout of the code. From that folder:

```sh
bri-import-addon port "C:/path/to/Weapon_Example.zip" "C:/path/to/Weapon_Example-port"
```

This sets up a work folder:

| Path | What it is |
|---|---|
| `AGENT.md` | the instructions, filled in for this Add-On, with a prompt to paste into your agent |
| `imported/` | the plain import, with `IMPORT-REPORT.md` and `import-report.json` |
| `original/` | the Add-On's own scripts, to read (never submitted) |
| `port/port.json` | the port, already drafted where it can be (below) |
| `port/checks.json` | what v20 does, which the check tests |
| `entry.json` | the Add-On's line in the ports list: the functions the port covers and patterns their v20 bodies must match |
| `stubs.rhai` | one stub per function still to port, quoting its v20 source, its hook and what it needs |

For weapons whose `onFire` uses v20's common spread code, the port is
drafted completely: the command prints `drafted:` for each one, and there is
nothing to write. Everything else is listed as `to port by hand:`. Hand
`AGENT.md` to your agent, or follow it yourself.

When the port is written, check it:

```sh
bri-import-addon check-port "C:/path/to/Weapon_Example-port"
```

It imports the Add-On again with your port, fires each weapon in
`port/checks.json` once and compares what happens with what v20 does
(projectiles per click, recoil, widest spread), and lists any function still
unported. When every check passes, it prints the entry for the ports list and
writes it to `submit.json`: `verified` when every function is ported,
otherwise `partial`, with this copy's hash. To submit, add the entry to
`crates/addon-import/ports/ports.json` and copy `port/` to
`crates/addon-import/ports/<port>/`, in a pull request or by handing both
files to someone who can.

Write each check from the v20 script, not from your port: it is the proof
that the port behaves like v20. A check with no numbers filled in fails.

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
| `tests` | the port's own checks (`<port>/checks.json`, which `check-port` runs) and any `path test_name` in the repository. The list's own test checks that they exist. |

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

## The recipe in detail

This is what `port` and `check-port` do for you, step by step, and what to
add when you work in a checkout.

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
   | An image's `onFire` (or charge, release, jet, light, wheel or cancel) or a `serverCmd` that does host work in an Add-On with weapons, vehicles or bricks | host rules (below): `rules/` in the port, and a patch pointing the image at their commands |
   | Anything whose `runtime_hook` is null or that needs a missing capability | not portable yet: port the rest, mark the entry `partial`, and say what is missing in `notes` |

   Keep the Add-On's own data where the importer put it. A port changes what
   the scripts changed, nothing else.

4. **Write the patterns** for every number or name the port relies on. Match
   the script's own spelling loosely (`\s*` around `=`), and capture the
   value, not the whole line.

5. **Write `ports/<port>/port.json`** and any `files/`.

6. **Prove it behaves like v20.** Fill in `port/checks.json` and run
   `check-port`. In a checkout, also add a test to
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

`AGENT.md` in the work folder holds the prompt, filled in for the Add-On.
Give your agent the folder and that file. In a checkout, point it at this
page as well.

## Ports so far

| Add-On | Port | Status | What it covers |
|---|---|---|---|
| `Weapon_Shotgun` (Sawn-off Shotgun) | `weapon_shotgun` | verified | `shotgunImage::onFire`: the pellets, their spread and the recoil, read from the copy's own script |
| `Weapon_ModernWarbattles` (Bushido's Adventurer's Weapons) | `weapon_modernwarbattles` | verified | the hl2 ammo system (magazines, reserves, reloads, ammo boxes, spare guns), every gun's shot from its own `onFire` (the Heavy Machine Gun's three fire states), the light key falling through to the light, the hitscan guns, their crits and shoves while `Emote_Critical` is on (its burst and sounds), the melee swings (players and vehicles, kill messages, hit sounds), headshots, the frag grenade's cooking, countdown and shrapnel, and a head hit's flinch |
| `Weapon_AdventurePack` (the Glass 1019 release) | `weapon_adventurepack` | verified | the same ammo system with its own reserves, its shots (the Paired Shotgun's single barrel), hitscan guns, headshots and the taser's tumble, sharing the rules above |

## Host rules

An imported Add-On is one `shared` package: its items, images and bricks go
to every player, and a shared package cannot carry host code. A port that
needs a host rule as well puts it in `ports/<port>/rules/`, and the importer
writes it as a second Add-On beside the import that only the host loads:

```
ports/<port>/
  port.json      { "schema_version": 1, "rules": { "capabilities": ["player", "world.edit"] }, "patch": { ... } }
  rules/
    behaviour.json
    <name>.rhai
```

| | The import | Its rules |
|---|---|---|
| Folder | `addons/<ns>` | `addons/<ns>-rules` |
| Id | `<ns>`, from the Add-On's folder name (`Tool_FillCan` is `tool_fillcan`) | `<ns>-rules` |
| Side | `shared` | `server`: players never download it |
| `package.json` | the importer's, with `"companions": ["<ns>-rules"]` | written for it: your `capabilities`, `behaviour` and `script` provides, and `dependencies` on the import at its version |

Turning the import on in the Add-Ons screen turns its rules on after it, and
turning it off turns them off. The importer checks the rules as the game
loads them (the manifest, `behaviour.json`, the script it names) and, as
with any patch, applies all of the port or none of it. The report lists the
rules under `ports[].rules`.

**Names.** In rules files, `{{name}}` becomes a value at import:
`{{namespace}}` (the import's id), `{{rules}}` (the rules' id),
`{{version}}`, or anything a `covers` pattern captured. A `{{word}}` that
names nothing is an error. The importer's ids are
`<ns>:<kind>/<datablock name in lower case>`, so a rule gives out the
imported item as `"{{namespace}}:weapon/fillcanitem"`.

**Reaching the rules.** In the patch, `{namespace}`, `{rules}` and
`{version}` work like captured values, in keys too. Point the image's
moments at the rules' commands ([Making Add-Ons](README.md) section 5,
`command` and `commands`):

```json
"assets/weapons.json": { "images": { "{namespace}:image/fillcanimage": {
  "command": "{rules}:fill",
  "commands": { "states": { "oncharge": "{rules}:charge", "onabortcharge": "{rules}:release" } }
} } }
```

`command` is `onFire`: it runs the rules' `cmd_fill(player)`, aimed where
the holder looks, instead of firing a projectile. `commands.states` runs a
command on entering any state whose script is that name; `jet`, `light`,
`wheel` and `cancel` are the other keys while it is in hand. The rules'
`behaviour.json` declares each command by the name after the colon, with
its `aim_reach` and `cooldown_ticks`.

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

## Magazines from item fields

Many v20 gun packs keep their magazines in item fields that a shared ammo
script reads: Jack's hl2 ammo system gives each item `maxmag` (rounds) and
`ammotype` (the reserve it loads from), and keeps the reserve sizes per
type. A port declares the convention once in `port.json`, and every item
with both fields gets a [`magazine`](README.md) on its image, from that
copy's own items:

```json
"magazines": {
  "size": "maxmag",
  "ammo": "ammotype",
  "reload_ticks": 120,
  "types": {
    "Pistol": { "ammo": "pistol", "reserve": 32, "max_reserve": 64 }
  },
  "items": { "huntingShotgunItem": { "one_by_one": true } }
}
```

| Field | Meaning |
|---|---|
| `size`, `ammo` | the item fields holding the magazine size and the ammo type's name (inherited fields count) |
| `types` | each ammo type by the name the items give it: the engine's `ammo` name, the starting `reserve` and the `max_reserve`. An item naming a type not listed stops the port, so a copy with other ammo is named in the report rather than guessed |
| `reload_ticks` | the reload's length when the image's states do not show it |
| `one_by_one` | the state script that loads one round (`onReloadSingle`): images with a state running it reload a round at a time, each round lasting from that state back to it |
| `every` | magazine fields for every gun (`"light_states": ["Ready", "Empty"]`), before `items` |
| `items` | extra magazine fields for one item, by datablock name |

The reload lasts as long as the image's own reload states: from the state
its ready state goes to without ammo, along each timeout, up to the state
that checks the ammo again. The rounds arrive as that check runs, so the
original animation and sounds play once and end with a full magazine. The
ammo display shows the type's name as the items spell it.

The rules get two values: `{{magazine_items}}`, a Rhai map from each gun's
item id to its engine ammo name, and `{{magazine_types}}`, from each type's
name to `#{ ammo, reserve, max_reserve }`.

## Tables of datablock fields

Host rules often need a number every datablock of a kind carries, such as
each projectile's `headshotMultiplier`. `"rules": { "tables": { ... } }`
reads them from the copy's own datablocks, and `{{name}}` in a rules file
becomes a Rhai map:

```json
"tables": {
  "headshots": {
    "class": "ProjectileData",
    "fields": ["headshotMultiplier"],
    "when": ["headshotMultiplier"],
    "key": "damage_type"
  }
}
```

| Field | Meaning |
|---|---|
| `class` | the datablock class |
| `fields` | the fields each row holds, in lower case in the map (`#{ "headshotmultiplier": 1.5 }`) |
| `when` | only datablocks where each of these is set and not `0` or `false` |
| `key` | `id` (the imported id, `<ns>:weapon/<name>`), `name` (the datablock's name) or `damage_type` (a projectile's damage type as `on_damage`'s `info.type` names it) |

A rule then reads `headshots()[info.type]` from `fn headshots() { {{headshots}} }`.

## Shots read from the scripts

`"shots": {}` reads every image's `onFire` written with the spread code
(`%projectile = ...; %spread = ...; %shellcount = ...;` and its loop) and
gives the image its `shot`. Each block is a set of projectiles; one with a
`setVelocity` recoil starts a shot, and the blocks after it are its
`volleys` (a shotgun's slug after its pellets). A `scale = "x y z"` on the
projectiles it makes becomes the shot's `scale`.

| Field | Meaning |
|---|---|
| `pick` | for an `onFire` that fires one of several shots (a branch per magazine count), which to port, by image name: 0 for the first. Several and no pick stops the port |
| `last` | `{ "<image>": { "shot": 1, "rounds": 2 } }`: which of its shots the magazine's last few rounds fire (a two-barrel gun's single barrel), as the image's `last_shot` |

The reader also takes from each state's own script what a state can say:
the sound it played (`serverPlay3d`), its arm move (`playThread(2, ...)`,
the state's `arm`) and the other hand's (`playThread(3, ...)`, its
`gesture`), and the camera shake of a recoil blast it set off at the
shooter (`spawnExplosion` of a projectile whose explosion shakes), as the
shot's `kick`. A fire state whose script is not `onFire` (`onFire2`) and
has the spread code becomes one of the image's `state_shots`. A
projectile whose own `damage` method dealt `directDamage` without reading
its scale gets `fixed_damage`.

## Hitscan guns from image fields

Raycasting support scripts gave each image fields for its ray. `"hitscans"`
names them, and each image whose `when` field is set gets `shot.hitscan`
and a projectile of its own (`<ns>:projectile/<image>ray`) carrying the
image's damage, so `on_damage` and tables see the ray by id (the ray is a
definition whose parent is the image, so a table of `ProjectileData` reads
the image's fields on it).

| Field | Meaning |
|---|---|
| `when`, `range` | the field that makes the image hitscan (or its range when there is no switch), and the range in units |
| `from_eye` or `from_muzzle` | the field that casts from the eye, or from the muzzle when set |
| `damage`, `damage_limit`, `damage_type` | the damage field, the script's own clamp, and the damage type field |
| `hit_projectile` | the field naming the projectile whose look and blast a hit shows |
| `impulse`, `vertical` | the shove along the shot and straight up |
| `count`, `spread`, `spread_degrees` | rays per shot and their spread (in degrees across with `spread_degrees`) |
| `tracer` | `{ "field": ..., "look": { "color", "width", "seconds" } }`: a streak for images where the field is set |

## Rules shared between ports, and their values

Two releases of one Add-On can share a rules script: `"rules": { "from":
"<port>" }` uses that port's `rules/` folder. What differs between them
goes in `"values"`: each becomes `{{name}}` in the script as a Rhai
literal, after `{capture}`s in it are filled, so a value can be built from
what the patterns read (`"{namespace}:sound/{baton_sound_a}"`).

When the scripts used another Add-On's datablocks only if it was there
(`if(isObject(CritProjectile))`), list it in the rules' `"uses"` by its v20
folder name (`["Emote_Critical"]`). The rules then name it in
`optional_dependencies`, `{uses:Emote_Critical}` in a value is its import's
id, and the script asks `enabled(...)` before using its content. When the
Add-On loaded the other one itself (`exec("add-ons/Emote_Critical/
server.cs")`, as ModernWarbattles did), list it in `"requires"` instead:
the rules then name it in `dependencies`, so turning the Add-On on turns
its import on too, and `{uses:...}` names it the same way.
