# Package runtime: gameplay from packages

Status: shipping; the Duplicator Add-On runs on it. Code:
`crates/package-runtime` (`bri-package-runtime`), `crates/sim/src/session/packages.rs`, the client's
`crates/client/src/packages.rs` and the UI's `hud.overlay` slot. Package
identity, `packages.json` and the join comparison are `bri-package`'s
([packages.md](packages.md)); this document covers what a mod package can
*do* once it is enabled.

The engine owns mechanisms; packages own policy. Nothing in these crates
knows about mining, ore, creatures or currency. Those live in the Stress
Lab packages (`packages/stresslab`), which use only the seams below.

## A mod package

A mod package is a directory listed in `packages.json` without a `role`,
holding its own `package.json` (the platform manifest shape: id, version,
api, license, provenance, dependencies, capabilities, provides). Each
`provides` entry names a content id in the package's namespace, a kind and a
file:

| Kind | Side | What it is |
|---|---|---|
| `behaviour` | server | Declared commands, state keys and hooks, and the script file. |
| `script` | server | Sandboxed server script (Rhai). |
| `world` | server | A chunked world provider: chunk size, voxel brick, materials (each may name a `block`), the generator function. |
| `entity` | server | An entity kind: name, model id, speed, scale, health, think function and interval, and optionally the `archetype` its body moves by. |
| `archetype` | server | A player archetype: a base plus the movement it changes, health, riding, look. |
| `model` | client | A box model: coloured boxes, colours per entity label. |
| `hud` | client | A HUD panel: title, colours, rows bound to public state, keys bound to commands. |
| `texture` | client | A PNG image (at most 1024 pixels on an edge), downloaded with the package. |
| `block` | client | A block: per face (`all`, `side`, `top`, `bottom`, `north`, `south`, `east`, `west`; the most specific wins) a texture id or a flipbook (`frames`, `fps`, `once`), and named `states` that replace some faces. |
| `mode` | server | A game mode: name, description, optional map, and the Add-Ons it runs. |

**Clients never receive or run package code.** Server kinds may only appear
in packages listed with `"side": "server"`; models and HUD panels only in
`client` or `shared` packages. A package that breaks this is refused with
`package.side.server_content` or `package.side.client_content`. Clients load
only their `client`/`shared` packages; the server tells them everything
else through replicated data (entities carry their model id).

## Game modes

Start Game's **Game Mode** button (above Start) opens a list, like v21's
gamemodes: **Custom** first, then every `mode` an enabled Add-On declares,
sorted by name, with its description and map. The choice is the
`$Pref::Server::GameMode` pref (empty for Custom). A mode that names a map
locks the map list to it.

```json
{ "schema_version": 1, "name": "Stress Lab",
  "description": "Dig through layered ground for ore ...",
  "map": "stresslab-world:world/strata",
  "add_ons": ["stresslab-world", "stresslab-economy", "stresslab-creeper", "stresslab-hud"] }
```

`add_ons` may name only the mode's own package and its direct dependencies
(`set.mode.add_on`), so turning the mode on turns on what it runs. A `map`
with a `:` must be a world one of those Add-Ons (or their dependencies)
provides (`set.mode.map`); a plain map name is a base map. Name, 1–48
characters; description, up to 512; up to 64 Add-Ons.

What a host runs (`crates/client/src/packages.rs` `hosted`):

- **A mode** runs its package, its `add_ons` and their dependencies
  (`Catalog::for_mode`) on its map, or on the selected map when it names
  none. Its state saves under `<mode>-<map>`.
- **Custom on a package world** runs every enabled Add-On except those that
  need a different world (`Catalog::for_world`), as before modes existed.
- **Custom on a base map** runs the enabled Add-Ons that need no package
  world and that no game mode claims (`Catalog::for_base_map`), such as
  the Duplicator.

Several world providers may be enabled at once; only a hosted game must
settle on one (`set.world.conflict`, raised by `for_mode`/`for_world`).
`packages/stresslab/stresslab-mode` is the worked example.

## The seams

| Seam | Family | Engine side | Package side |
|---|---|---|---|
| Package commands | player/control, security | `Command::Package { package, command, args }`: checked against the declared name, argument types, cooldown and admin flag; optional server-resolved aim. | `fn cmd_<name>(player, args...)` |
| Package state | persistence, game-rule | Namespaced per package; per durable player (principal) and per server; only declared keys; committed only when a call succeeds; keys replicated to the audience their `visible` names; persisted keys saved (`PackageSave`). | reads and writes its own keys |
| Entities | entity/behaviour | Character bodies (motor `spawn_tagged`, collider kind 3), steering, labels, health, fall-out removal, replication as `EntityInfo`. | `fn think(me)` every N ticks: `steer`, `label`, `explode`, ... |
| Chunked world provider | world, persistence | Streams chunks around players, inserts world-owned bricks, records removals (by any means) as edits, saves seed + edits, regenerates on load. | `fn generate(cx, cz)` returns `[[x, y, z, material], ...]` |
| Explosion | game-rule | `Session::explode`: player and entity damage with falloff, brick destruction in a radius, `Explosion` cue. | `explode(x, y, z, r, damage, brick_r)` |
| HUD slot `hud.overlay` | UI | Panels drawn from data, values from replicated public state, key hints; base-game binds win. | `hud` JSON |
| Box models | UI | One cube instanced per box, tinted, following entity pose and label. | `model` JSON |

Hooks: `on_join(player)`, `on_tick()` every `tick_interval` ticks,
`on_death`, `on_loadout`, `on_spawn`, `on_leave`, `on_damage`,
`on_entity_damage` and `on_entity_death` (the modding guide's section
3).

## Operations and the capability gate

Scripts never touch the game. A call receives a read-only snapshot (tick,
seed, players, entities, the caller's aim) and a working copy of its own
state; it returns a list of typed operations (`bri_package_runtime::Op`):
`remove_brick`, `place_brick`, `explode`, `damage`, `teleport`, `respawn`,
`set_archetype`, `control`, `set_block_state`, `spawn_entity`,
`remove_entity`, `steer`, `label`, `tell`, `broadcast`, `give_item`
(capability `player`) and `copy_build`, `copy_box`, `mirror_copy` and
`highlight_copy` (capability `build`: the engine copies a stack of the
caller's build (`Simulation::select_stack`), or a box of it, as the
Add-On's `CopyRule` allows, into a blueprint, `crate::blueprint`, that the
player places with `Command::PlaceBlueprint` under the plant rules, all or
none or brick by brick as the rule asks, with one undo entry; `on_copy`
and `on_place` tell the Add-On how it went, and `on_copy_ghost` where the
copy stands as its player places it (`Command::CopyPose`, the box from
`Blueprint::ghost_box`); highlights recolour the
bricks for a while, as v20 did (`session::highlight`); mirroring is part of the placement, with twins found by
`crate::mirror`; `save_copy` and `load_copy` keep blueprints by name in
the host's `session::CopyStore`, which answers off the tick thread (the
client's `copies` module: files in `saves/Duplications`, and v20
duplication files read through `bri_bls::bls::read_duplication`)), `cut_copy` and `paint_copy` (capability `world.edit`:
the copy's originals, with the caller's full trust, each one undo entry),
the physics operations (capability `physics`),
`heal` and `fire` (capability `damage`: `fire` launches a projectile of
the package's weapons or a dependency's, 240 a second), `center_print`
and `bottom_print`
(capability `chat`), `set_fov`, `set_image_ammo` and `mount_image`
(capability `player`), and `play_sound`, `sound_at`, `beam` and
`play_thread` (capability `effects`: presentation only, each one cue
within the package's cue allowance), `show_box` and `hide_box`
(`effects` too: one player's selection outline), and `show_shapes` and
`hide_shapes` (`effects`: boxes every player sees, replicated by key as
`Checkpoint::world_shapes` and `Delta::world_shapes`). `damage` takes a player or any
object and an optional weapons-pack damage type. `set_block_state(brick,
state)` (capability `world.edit`) switches a block brick to one of its
block's declared states; the state is a field of the brick
(`Brick::look`), so it replicates and saves with the world. `aim()` reports
the aimed brick's `block` and `state`. `control(player, entity)` hands a player's
movement to one of the package's own entities, `release(player)` hands it
back (capability `player`; see `docs/player-simulation.md`).

Two questions read the live world during a call instead of the snapshot:
`raycast` (the weapons' own sweep, at most 64 rays of 2000 units per call)
and `can_damage` (the minigame damage policy). They need no capability,
like every read. The session hands the runtime a `script::World` for the
call; `Runtime::call` takes `&self` and enforces the operation budget in
the engine's progress callback, so the session is only borrowed for
reading while a script runs. Chunk generation passes no world.

Every operation passes **`ops::authorize`**, the single capability gate:
bounds first (`op.bounds`), then the capability the manifest declares
(`world.edit`, `damage`, `entity`, `chat`; `op.capability`), then
`op.foreign_entity` (packages spawn only their own kinds). The session then
checks ownership (`op.not_owner`: a package steers only its own entities).
If any operation or state write fails, the whole call is discarded.

### Adding an operation

Each operation is a struct implementing `ScriptOp` (its capability, its
name and its limits, `bounded`) in the module of the capability it needs:
`crates/package-runtime/src/ops/<capability>.rs` (`player.rs`,
`build.rs`, ...). `ops/list.rs` names every operation once, as a
`Variant = module,` line, and a new capability adds its `pub mod` line
there. The `Op` enum, `capability()`, `name()` and the limits check are
generated from that list, so no central match needs editing. Git merges
`list.rs` with the union driver (`.gitattributes`), so two branches that
each add an operation never conflict there. Scripts ask for one with
`push(Op::YourOp(ops::YourOp { .. }))` in `script.rs`; the host applies it
in `bri-sim`'s `session/packages.rs`.

## The sandbox (Rhai, prototype)

Chosen for the probe because it is pure Rust, needs no compiler for the
agent, and is easy to bound; not a permanent commitment. Limits
(`script.rs`): operation budget per call (`Budget`: command 200k, think
100k, tick 400k, generate 400k), 32 call levels, expression depth, 4 KiB
strings, 64k arrays, 1k maps, 256 variables and functions, 1024 operations
per call. No `eval`, modules, clock, file or network. Scripts may only
define functions (`script.top_level`). Over budget is `script.budget`,
limits `script.limit`, other failures `script.error`, each with the file and
line. A failing entity think stops that entity for a second instead of
spamming.

Work the engine runs itself (thinks, `on_tick`, generation) shares 200k
operations a tick, split evenly between packages with scripts: about 8 ms
on a desktop. A package whose call ran past its share repays it over later
ticks, and its waiting thinks run first next tick.

## Add-On tools

A weapon image with `command` (`package:command`) is an Add-On tool: its
`onFire` runs that package command for the holder, aimed along the swing,
instead of firing a projectile (`WeaponEvent::ToolFire`).

## Replication

`Checkpoint` and `Delta` carry `entities: Vec<EntityInfo>`, sent whole when
they change at the 20 Hz delta rate. Package state is per client: a state key
declares `visible` (`server`, `owner` or `everyone`), the welcome's
`Checkpoint.package_state` is `Session::package_state_for(viewer)`, and the
server sends each client `Message::PackageState` with its whole view when
that view changes (protocol 35). One player's owner-visible keys never reach
another client. Whole views are fine for tens of entities and small state;
larger counts need per-entity deltas (see the Stress Lab handoff).

## Workflow

- `stresslab check [<root>] [--list packages.json]`: load, validate and
  compile; JSON diagnostics with stable codes.
- `stresslab test`: headless scenario run with a JSON report.
- `stresslab-soak --clients N --seconds S`: loopback host plus headless
  clients.
- Tests: `cargo test -p bri-package-runtime -p bri-stresslab`, and
  `cargo test -p bri-sim --test packages`.
