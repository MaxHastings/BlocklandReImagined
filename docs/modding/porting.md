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
`{name}` inside a longer string becomes its text, and `{name:lower}` its
text in lower case, for ids: Torque ignores the case of names
(`"weapon_example:projectile/{jab:lower}"`).

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
   | An image's state script (`onCharge`, `onFire`, a custom `stateScript` such as `onFiretwo`) that plays an arm animation, calls `Parent::onFire`, spawns a second projectile or uses the item up | an entry in the image's `scripts` ([torque-equivalents.md](torque-equivalents.md#image-state-scripts-as-data)) |
   | Something the game already does the same way | nothing: cover the function with patterns and say so in `notes` |
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
| `Tool_Duplicator` (Plornt's Duplorcator) | `tool_duplicator` | partial | `/dup`, `/duplorcator`, `/duplicator`; `DuplorcatorImage::onFire` (reach, full trust, no public bricks, selection wait); `getStack` (up from the clicked brick, every way from the rest; the cyan highlight and how long it lasts); planting brick by brick with its count, one undo; `/saveDup` and `/loadDup` (v20 duplication files load too). Not ported: uploading a duplication from the player's computer |
| `Tool_NewDuplicator` (Zeblote's New Duplicator) | `tool_newduplicator` | verified | its preference defaults and `$ND::Version`; `/newduplicator` and `/duplicator` down to `/d`; stack and box selection (direction, limited, box corners, its 64 and 1024-unit box limits, select wait); the mode images and their mount handling; plant mode with its planted, blocked, floating and missing-trust counts, the pivot ([Prev Seat]), `/PlantAs`, the plant wait and the big-undo question; clicking to move a selection; `/MirrorX`, `/MirrorY`, `/MirrorZ` (up and down), `/MirErrors`, `/Cut`, `/SaveDup` (with its overwrite warning), `/LoadDup`, `/AllDups`, `/DupVersion`, `/DupClients`, `/ClearDups`, `/DupHelp`; its keys (Ctrl C, V and X, Ctrl held to multiselect, Shift-Ctrl X and V, and every Send entry, under New Duplicator in Controls); force plant and `/ForcePlant`, fill colour (spray and FX cans on a selection), `/FillWrench`, `/SuperCut` and `/FillBricks` with their confirm questions, the selection box from a selection; `ndFormatMessage`. Its 10,000-brick player limit and 1,000,000-brick admin limit, with each big job's progress bar, `[Cancel Brick]` and `% Ghosted` (below) |
| `Weapon_Sniper_Rifle` (Kaje's Sniper Rifle) | `weapon_sniper_rifle` | verified | `SniperRifleImage::onFire`: the arm's kick then the shot (`scripts.onfire`), the animation's name read from the copy's script |
| `Weapon_Sniper_Rifle_Updated` (Conan's Sniper Rifle Updated) | `weapon_sniper_rifle_updated` | verified | `onFire`'s `plant` then the shot (`scripts.onfire`); `onMount` hiding the holder's hands and hooks and raising both arms, and `onUnMount` putting them back (`hide_nodes`, `both_arms`) |
| `Gamemode_TrenchDigging` (Trench Digging, Lilboarder) | `gamemode_trenchdigging` | verified | Every function of `TrenchDigging.cs` and the four images' `onPreFire`/`onFire`, as host rules (`rules/trench.rhai`): dig, put back, regroup, `/dumpdirt`, `/speeddig`, `/speedplace`, `/infinitedigging`; `server.cs` raising No Jet's `maxStepHeight` to 1.2 is `rules/archetypes/playernojet.json` |

### How a million-brick copy keeps the server running

The New Duplicator let admins select up to 1,000,000 bricks. It could,
because it never did a big job at once: it selected, planted, cut, painted
and saved a few hundred bricks a tick (`ProcessPerTick`, 300) behind a
progress bar, and showed only some of them as the ghost
(`MaxGhostBricks`). The engine does the same with copy jobs
(`crates/sim/src/session/copy_jobs.rs`). Selecting, planting, cutting,
painting, wrenching, loading and undoing a copy each take a slice of the
tick's copy work (about 2.5 ms of a release build), shared by every
player with a job in turn, so one player's huge copy never holds up the
server or the others. A job that fits in the slice still finishes within
the command. While it runs, the player's duplicator hears how far it has
got (`on_copy` with `working`), its other copy work is refused as busy,
and `cancel_copy` stops it: what it did by then stays done, as one undo
step. The player's game gets at most 10,000 bricks of the copy, spread
through it, for the ghost (the port shows the `% Ghosted` the original
did); the whole copy stays on the host. So the port keeps the original's
limits: 10,000 bricks for players and 1,000,000 for admins.

Measured on a release build (100,000 to 1,000,000 2x1 plates, at the
default copy work), no tick of a job went over 7 ms: planting 500,000
into a world of 500,000 took 2,202 ticks with the slowest at 3.3 ms;
undoing it, 1,251 ticks, 5.2 ms; cutting 1,000,000, 1,199 ticks, 4.4 ms;
putting them back, 4,906 ticks, 6.7 ms. A planted copy still counts
against the server's brick limit.

`/SuperCut` and `/FillBricks` are copy jobs too, with the original's
limits: only the box size (1024 units for admins, 64 for players) bounds
them, and the engine stops a box holding more than 1,000,000 bricks. A
supercut shows the original's "Supercut in progress... (N%, N deleted, N
planted)"; the original filled at once with no progress line, so the
port's "Filling in bricks... (N%)" is ours. A fill stops at the server's
brick limit and says how far it got. On 500,000 2x1 plates: a supercut
took 650 ticks, slowest 4.8 ms, and its undo 2,403 ticks, 5.5 ms; a fill
of 250,000 bricks took 1,654 ticks at 3.2 ms on average (its first two
ticks cost up to 25 ms as the physics first meets the box, every later
one under 6 ms), and its undo 2,870 ticks, 6.1 ms.

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
    archetypes/<name>.json   (optional) player archetypes, or adjustments to v20's
```

| | The import | Its rules |
|---|---|---|
| Folder | `addons/<ns>` | `addons/<ns>-rules` |
| Id | `<ns>`, from the Add-On's folder name (`Tool_FillCan` is `tool_fillcan`) | `<ns>-rules` |
| Side | `shared` | `server`: players never download it |
| `package.json` | the importer's, with `"companions": ["<ns>-rules"]` | written for it: your `capabilities`, `behaviour`, `script` and `archetype` provides, and `dependencies` on the import at its version |

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
