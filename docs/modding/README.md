# Making Add-Ons

This guide is for anyone who wants to make an Add-On for Blockland
ReImagined: a new weapon, a game rule, a HUD panel, a generated world or a
creature. It covers what works on `main` today and marks what is still being
built as **coming soon**, so you never write against an API that does not
exist yet.

Players only ever see the word **Add-On**. Inside the engine and in file
names an Add-On is a *package* (`package.json`, `packages.json`). They are the
same thing. Old Blockland v20 Add-Ons (`.zip` files) are brought in with
**Import Add-On**, described at the end.

Three working samples live in [`packages/samples/`](../../packages/samples):

| Sample | What it shows | Proven by |
|---|---|---|
| `sample-survival-points` | A server game rule: state, a timer, commands, an admin-only command, chat | `crates/sim/tests/samples.rs` |
| `sample-points-hud` | A HUD panel bound to the rule's state, with a key | `crates/package-runtime/tests/samples.rs` |
| `sample-bubble-blaster` | A weapon: item, image states, projectile, damage type | `crates/weapons/tests/sample_addon.rs` |

Copy one, rename it, and change it. That is the fastest way to start.

## 1. How an Add-On is shaped

An Add-On is one folder with a `package.json` manifest and the files it
lists:

```text
sample-survival-points/
  package.json      what it is, who made it, what it needs, what it provides
  behaviour.json    commands, state and hooks the engine calls
  points.rhai       the script those hooks run
```

The game's content root has a `packages.json` listing the Add-Ons that are
on. The in-game **Add-Ons** screen (main menu; landing with the Add-Ons
screen change now in review) turns them on and off for you:
it moves entries between `packages.json` and `packages-disabled.json`, turns
on dependencies first, and shows what each Add-On is allowed to do. See
[mod-manager.md](../architecture/mod-manager.md).

Two rules shape everything else:

- **Servers send data, never code.** Scripts run only on the server. A
  client gets declarative data: HUD layouts, models, state values. Your
  script is never downloaded to players.
- **The engine owns mechanisms; Add-Ons own policy.** The engine offers
  generic tools (state, timers, commands, entities, chat, brick removal).
  What they mean (points, money, ore, rounds) is entirely yours.

## 2. The manifest: `package.json`

```json
{
  "schema_version": 1,
  "id": "sample-survival-points",
  "version": "1.0.0",
  "api": 1,
  "name": "Survival Points",
  "description": "Every living player earns a point every five seconds.",
  "authors": ["You"],
  "license": "CC0-1.0",
  "provenance": { "source": "original" },
  "dependencies": {},
  "capabilities": ["chat"],
  "provides": [
    { "kind": "behaviour", "id": "sample-survival-points:behaviour/points", "file": "behaviour.json" },
    { "kind": "script", "id": "sample-survival-points:script/points", "file": "points.rhai" }
  ]
}
```

| Field | Rules |
|---|---|
| `id` | 1-64 characters: `a-z`, `0-9`, `-`, `_`; starts with a letter; does not end with `-` or `_`. It never changes once people use your Add-On. `v20` and ids starting `v20-` are reserved for the game. |
| `version` | Semantic version, `MAJOR.MINOR.PATCH`. |
| `api` | The engine API level you wrote against. Today `1`. |
| `name` | 1-64 characters, shown in the Add-Ons screen. |
| `license` | Required. Say what others may do with your work. |
| `provenance.source` | `original` for your own work. Imports record where they came from. |
| `dependencies` | Other Add-On ids and a version requirement such as `"^1.0.0"`. The Add-Ons screen turns them on for the player. |
| `capabilities` | What your scripts may do to the world (section 4). |
| `provides` | Every content file, each with a `kind` and a content id `your-id:kind/name`. Content ids are how everything refers to everything else. |

## 3. Sides: server, client, shared

Each entry in `packages.json` has a `side`:

- **server**: behaviour, scripts, worlds and entities. Only the host loads
  them. Players do not need them installed.
- **client**: HUD panels and models. Each player loads them.
- **shared**: both need the same copy, for example a weapon or new bricks.
  When a player's shared Add-Ons differ from the server's, the join is
  refused and the **Can't Join** screen lists each Add-On with the server's
  version and theirs.

The Add-Ons screen picks the side for Add-Ons it discovers: `server` when
everything provided is behaviour, script or world, otherwise `shared`.

## 4. Scripts, commands, state and capabilities

Game rules are a `behaviour` file plus a Rhai `script`. The sample's
`behaviour.json`:

```json
{
  "schema_version": 1,
  "script": "points.rhai",
  "commands": [
    { "name": "top", "cooldown_ticks": 240 },
    { "name": "reset", "admin": true }
  ],
  "state": {
    "player": {
      "points": { "default": 0, "public": true },
      "best":   { "default": 0, "public": true }
    },
    "global": {
      "awarded": { "default": 0, "public": true },
      "every":   { "default": 1, "persist": false }
    }
  },
  "on_join": true,
  "tick_interval": 600
}
```

**Hooks.** A script may only define functions; top-level statements are
refused. The engine calls:

| Function | When |
|---|---|
| `on_join(player)` | a player joins, when `"on_join": true` |
| `on_tick()` | every `tick_interval` ticks (120 ticks = 1 second) |
| `cmd_<name>(player, args...)` | a player sends a command listed in `commands` |

Compiling checks that each of these exists with the right number of
parameters, so a typo fails when the Add-On loads, not mid-game.

**Commands** are the only thing a player can ask of your script. Each
declares its argument types (`int`, `float`, `string`, `bool`), an optional
`cooldown_ticks` per player, `admin: true` to refuse non-administrators, and
`aim_reach` to have the engine resolve what the player is aiming at (read it
with `aim()`).

**State** is declared up front with defaults. `player` keys exist for every
player; `global` keys once per server. `public: true` sends the value to
every client (HUD panels can only show public keys). `persist` (default
`true`) saves the key with the host's world and restores it after a restart.
Private keys never leave the server.

**Script functions:**

| Read | Change state | Act on the world (needs capability) |
|---|---|---|
| `tick()`, `seed()`, `caller()` | `get(key)`, `set(key, value)` | `tell(player, text)`, `broadcast(text)`: `chat` |
| `players()`, `player(id)` | `get_player(p, key)`, `set_player(p, key, v)` | `remove_brick(brick)`: `world.edit` |
| `aim()`, `me()`, `entities()` | `add_player(p, key, amount)` | `damage(p, amount)`, `explode(...)`: `damage` |
| `noise(seed, x, z)`, `hash3(seed, x, y, z)` | `entity_get(e, key)`, `entity_set(e, key, v)` | `spawn_entity`, `remove_entity`, `steer`, `label`: `entity` |

A player value from `players()` is a map with `id`, `name`, `x`, `y`, `z`,
`alive` and `admin`.

**Capabilities** are the only permission gate. If your script calls
`tell` without `"chat"` in `capabilities`, the call is refused with a
message telling you what to add. The Add-Ons screen shows players the
capabilities in plain words ("send chat messages", "change the world's
bricks"), so ask
for only what you use. Scripts also run inside budgets: operations per call,
string, array and map sizes, call depth. A runaway loop stops with a
diagnostic instead of freezing the server. Scripts cannot read files, open
sockets or run `eval`.

## 5. Content kinds

| Kind | Side | File | Example |
|---|---|---|---|
| `behaviour` | server | commands, state, hooks | `packages/samples/sample-survival-points` |
| `script` | server | Rhai functions | same |
| `world` | server | a generated chunk world: materials, a `generate(cx, cz)` function | `packages/stresslab/stresslab-world` |
| `entity` | server | a scripted creature: model, `think` function, speed, health | `packages/stresslab/stresslab-creeper` |
| `model` | client | a box model for an entity | `packages/stresslab/stresslab-creeper-model` |
| `hud` | client | a HUD panel | `packages/samples/sample-points-hud` |

Entities may spawn only their own Add-On's entity kinds.

### Weapons

A weapon is an `assets/weapons.json` file in the same format **Import
Add-On** writes for v20 weapons: items, images with their state machine,
projectiles, damage types and explosions, each keyed by a content id. The
Bubble Blaster sample is a hand-written one; its `provides` is empty because
weapons are not a `provides` kind yet. Model paths may point at base game
models (the sample reuses `Add-Ons/Weapon_Gun/pistol.dts`).

**Coming soon:** loading an Add-On's weapons, bricks and sounds into a hosted
game arrives with multi-pack loading. Until then a weapons file is
checked by the weapons runtime in tests, as the sample does.

## 6. HUD panels

A HUD panel is data. The client draws it from public state:

```json
{
  "schema_version": 1,
  "slot": "hud.overlay",
  "anchor": "top_left",
  "title": "SURVIVAL POINTS",
  "background": [0.05, 0.08, 0.12, 0.8],
  "accent": [0.35, 0.85, 0.45, 1.0],
  "text": [0.92, 0.95, 1.0, 1.0],
  "rows": [
    { "label": "Points", "bind": "sample-survival-points:player/points" },
    { "label": "Awarded to everyone", "bind": "sample-survival-points:global/awarded" }
  ],
  "keys": [
    { "key": "J", "label": "Leaderboard", "package": "sample-survival-points", "command": "top" }
  ]
}
```

- `bind` is `add-on-id:player/key` (the viewing player's value) or
  `add-on-id:global/key`. The key must be declared `public`, or the row stays
  blank. The sample test checks this for you.
- `anchor` is `top_left`, `top_right`, `bottom_left` or `bottom_right`.
- Up to 16 rows and 8 keys. A key is one letter `A`-`Z` that sends a command
  with no arguments. The client refuses letters the base game already uses.
- Colors are RGBA from 0 to 1.

**Coming soon:** the client drawing Add-On HUD panels and sending their
keys lands with the Stress Lab hosting work. On `main` the panel is loaded
and validated, and the state it binds to is replicated.

## 7. Testing your Add-On

Everything is testable without opening the game. The samples show three
levels; copy whichever fits:

1. **Loads and compiles**: `Catalog::load(root, &set, server)` then
   `Runtime::compile(&catalog)`. This catches manifest mistakes, bad content
   ids, missing hook functions and script syntax errors, each with a file,
   a code and a hint. See `crates/package-runtime/tests/samples.rs`.
2. **Runs in a real session**: build a `Session`, call
   `install_packages`, `join` players, `step` ticks, send
   `Command::Package(...)`, and read `package_value`, `package_state` and
   `chat`. See `crates/sim/tests/samples.rs`.
3. **Weapons**: `Pack::from_json` then `WeaponsWorld` to give, equip and
   fire. See `crates/weapons/tests/sample_addon.rs`.

```sh
cargo test -p bri-package-runtime --test samples
cargo test -p bri-sim --test samples
cargo test -p bri-weapons --test sample_addon
```

`package_diagnostics()` lists any problem your script hit at run time
(a refused capability, a budget overrun), so assert it is empty.

**Coming soon:** a `stresslab` command-line tool that loads a folder of
Add-Ons and runs them headless without writing Rust, and hosting an
Add-On game from the Start Game screen.

## 8. Importing v20 Add-Ons

Old Blockland Add-Ons are `.zip` files or folders with `server.cs` and
friends. The importer converts one into a native Add-On folder without ever
running its scripts:

```sh
cargo run -p bri-addon-import --bin bri-import-addon -- \
  Weapon_Example.zip out/weapon_example --reference "<v20 folder>"
```

It writes `package.json`, `assets/weapons.json`, converted bricks and
textures, and `IMPORT-REPORT.md` saying what came across and what did not
(TorqueScript logic is not run; only datablocks become data).

**Coming soon:** an **Import Add-On** button on the Add-Ons screen that does
this for Add-Ons dropped in the game's `Add-Ons` folder.

## 9. Still being built

These are designed but not on `main` yet. Write against them once they land;
this guide will be updated in the same change.

- **Multi-pack loading**: an Add-On's weapons, bricks and sounds in a
  hosted game.
- **State visibility**: `visible` on state keys, `server`, `owner` or
  `everyone`, instead of `public`, so a player can see their own private
  values.
- **The `players` capability**: moving and changing players from scripts.
- **Player archetypes**: Add-Ons that define their own playable
  characters and controllable vehicles, with their own movement, body,
  camera and model.
- **Sandboxed client code**: Add-Ons with client-side logic and shaders,
  run in a sandbox. Joining a server that uses them asks the player to
  trust that server first, listing what the code may do.
- **Downloads on join**: fetching a server's missing shared Add-Ons while
  joining, with progress, instead of refusing.
