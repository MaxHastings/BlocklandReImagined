# Making Add-Ons

This guide is for anyone who wants to make an Add-On for Blockland
ReImagined. It covers what works today; anything still being built is in
the last section, so you never write against something that does not exist
yet.

Read it in order: make an Add-On (sections 1 and 2 here, then the pages
for rules, HUD panels, weapons and other content), bring in an old v20
Add-On and port what its scripts did ([Old v20 Add-Ons](old-addons.md)),
then what players are asked to trust when your Add-On runs code on their PC
([trust](trust.md)).

## What you can make

| You want | Start from | It is |
|---|---|---|
| A game rule: points, rounds, commands | [`sample-survival-points`](../../packages/samples/sample-survival-points) | a `behaviour` file and a Rhai script, run by the host |
| A framework other Add-Ons build on (an economy, a roleplay core), with data kept across restarts | [`sample-rp-bank`](../../packages/samples/sample-rp-bank) | a rule that exports functions and sends events ([Game rules](rules.md), **Add-Ons that build on each other**) |
| A window the rules open: a shop, a backpack and chest, an editor | [`sample-rp-inventory`](../../packages/samples/sample-rp-inventory) | `show_panel` and `on_panel`, with rows dragged between lists ([Game rules](rules.md), **Panels**) |
| Wiring and logic on bricks, in the spirit of Wiremod | [`sample-wire-lite`](../../packages/samples/sample-wire-lite) | a rule using aim, empty-hand clicks, brick colours and a circuit panel |
| A camera of your own, or taking over some controls | [`sample-overhead-camera`](../../packages/samples/sample-overhead-camera) | client code with `camera.control` and `input.capture` ([client sandbox](../architecture/client-sandbox.md)) |
| A HUD panel for a rule | [`sample-points-hud`](../../packages/samples/sample-points-hud) | a JSON panel each player draws |
| A weapon | [`sample-bubble-blaster`](../../packages/samples/sample-bubble-blaster) | an `assets/weapons.json` file |
| A tool that grabs, holds and throws players and vehicles | [`gravity-gun`](../../packages/showcase/gravity-gun) | a rule using the `physics` operations ([Game rules](rules.md)), its tool, and client effects |
| A new vehicle or loose physics object | [`steel-ball-kit`](../../packages/showcase/steel-ball-kit) | an `assets/vehicles.json` you write ([Other content kinds](content-kinds.md)) |
| A bot for the Vehicle Spawn brick | [`blockhead_bot`](../../packages/blockhead_bot) | an `assets/bots.json` you write ([Other content kinds](content-kinds.md)) |
| A package-owned bot pickup/return objective | [Experimental bot objectives](bot-objectives.md) | a bounded read-only declaration of existing pickup, carriage, zone and completion policy |
| Effects drawn on every player's screen | [`gravity-gun-fx`](../../packages/showcase/gravity-gun-fx) | WebAssembly and WGSL shaders reading what the game shows ([Other content kinds](content-kinds.md)) |
| New bricks | a v20-style brick Add-On you import ([Old v20 Add-Ons and new bricks](old-addons.md)) | a brick catalog the importer writes |
| A brick modelled in Blender: a table, a statue, a sign | [Models from Blender (glTF)](gltf-models.md#bricks) | `bri-import-addon gltf --kind brick` turns a `.glb` into a brick Add-On |
| A map: a level built in Blender, with spawn points and lights | [`sample-map`](../../packages/samples/sample-map) | `bri-import-addon gltf --kind map` turns a `.glb` into a map Add-On ([Making maps](maps.md)) |
| A game mode in Start Game | [`stresslab-mode`](../../packages/stresslab/stresslab-mode) | a `mode` file naming Add-Ons and a map |
| A game mode on a world players dig into, with its own mini-game | [`crates/sim/tests/mode_and_voxels.rs`](../../crates/sim/tests/mode_and_voxels.rs) | rules with a generated world, a tool and a mode whose `minigame` block runs the game ([Other content kinds](content-kinds.md)) |
| A character, weapon or vehicle model made in Blender | [Models from Blender (glTF)](gltf-models.md) | `bri-import-addon gltf` turns a `.glb` into a body Add-On or a model for `weapons.json` and `vehicles.json` |
| Worlds, creatures, bodies, blocks | [`packages/stresslab`](../../packages/stresslab) | see [Other content kinds](content-kinds.md) |
| A whole new game on top: bodies, scoped guns, creatures that shoot back, scoring | [`sample-commando`](../../packages/samples/sample-commando) and its four siblings | five Add-Ons: a weapon, a look with client code, rules, a HUD and a mode ([total-conversion.md](../audits/total-conversion.md)) |

Old Blockland v20 Add-Ons (`.zip` files) also work: see [Old v20 Add-Ons and new bricks](old-addons.md).

## 1. Make one

1. Copy the sample closest to what you want and give the folder a new name.
2. Pick an id for your Add-On: lowercase letters, digits, `-` and `_`,
   starting with a letter, for example `coin-rain`. It never changes once
   people use your Add-On. Replace the sample's id **everywhere** in the
   folder: in `package.json` (`id` and every `provides` id), and in any
   other file that names it (a HUD's `bind` and `package`, the ids inside
   `weapons.json`).
3. Change `name`, `description` and `authors`, then make it your own.

A weapon only needs a new id. A rule needs its commands and state changed in
`behaviour.json` and its script rewritten ([Game rules](rules.md)). Rules
are written in Rhai, a small scripting language; [Rhai in ten
minutes](rhai.md) teaches the parts rules use.

## 2. Check it and try it

You don't need Rust. The game's download carries two tools for this beside
the game itself: `bri-addon-check` and `bri-addon-run` (`.exe` on Windows;
on a Mac they are in `BlocklandReImagined.app/Contents/MacOS/`). Open a
terminal in the game's folder (on Windows, type `cmd` in the folder's
address bar) and run them from there. The samples are in this repository:
on GitHub, **Code > Download ZIP** gets them without git.

**Check** that the game can load it. Put the Add-Ons it depends on in the
same parent folder as it (for a HUD, next to its rule):

```sh
bri-addon-check path/to/coin-rain
```

(`.\bri-addon-check` in PowerShell, `./bri-addon-check` on Linux and Mac.)
In a checkout of this repository, `cargo run -p bri-package-runtime --bin
bri-addon-check --` and `cargo run -p bri-sim --bin bri-addon-run --` do the
same, built from your checkout.

It prints who needs the Add-On (the host only, each player, or everyone),
what it provides and may do, and every mistake with its file, line and a
hint: a misspelt field, a missing hook function, a script syntax error, a
HUD row bound to a key players never receive, a broken weapons file. Each
sound file a weapons pack ships is run through the game's own decoder, so a
sound that would load and then play silence (an unreadable header, a codec
the game does not play, such as ADPCM WAV) fails the check by name; the
game plays PCM WAV and Ogg Vorbis files of up to 30 seconds and 8 MiB.

**Try** a rule without opening the game. `bri-addon-run` runs it with the
Add-Ons it needs in an empty world with two players, `Host` (an admin) and
`Guest`, sends the commands you give it one second apart, and prints the
chat, the state players receive and any script problem:

```sh
bri-addon-run path/to/coin-rain --send coins --wait 20 --send "gift 1" --send "Guest: coins"
```

`--wait N` waits N more seconds before the next command, `--seconds N` runs
at least that long (10 by default), `"Guest: ..."` sends as the Guest and
`other-add-on:command` sends another Add-On's command.

**Play** it: put the folder in the game's `content/addons/` folder
(`content` beside `Launch.cmd` in a release folder), open **Start Game > Add-Ons**, and turn it on. The screen turns on what it
depends on and shows players what it may do. See
[mod-manager.md](../architecture/mod-manager.md).

**Not in your game?** An Add-On that brings rules, bodies, a HUD,
creatures, blocks or a world plays only in the games it fits; weapons and
vehicles play in every game. Pick it in the Add-Ons screen: a "Not in
every game" paragraph names the game mode or map that plays it, for
example a body a game mode uses, which a Custom game leaves out. In a
game, `/body` lists the bodies this game left out, and `/body <name>` says
why.

**Edit while you play.** A game keeps the Add-Ons it started with. Edit a
file while you host, switch back to the game, and chat names the Add-Ons
that changed; host again to play them. Two exceptions skip hosting again:

- **Rules scripts.** Type `/reloadrules` (the host only) after saving a
  `.rhai` file. Every running Add-On's script is read again and the
  changed ones swap in at once. Everything the rules keep carries on:
  state, host data, settings, creatures, cooldowns and the players, so
  the next hook simply runs your new functions. Nothing re-runs `on_join`
  for players already in. If the new script has a mistake, chat says what
  and where, and the old one keeps running. Only the script is read
  again: a change to `behaviour.json` (commands, state, hooks turned on)
  or any other file still needs hosting again.
- **A body's numbers.** `/tune` changes them live for everyone and saves
  them back to its file ([Tuning bodies in the game](tuning.md)).

**Share it** as one ZIP that installs byte for byte:

```sh
bri-addon-check path/to/coin-rain --package
```

It runs the check above first and packs nothing when the check fails. The
ZIP holds the Add-On's folder and its `coin-rain-rules` companion if there
is one, in a fixed order with fixed dates, so packing the same files twice
gives the same ZIP; it skips hidden files and `target` folders, then reads
the ZIP back and compares every file before it is kept. `--out FILE.zip`
names it (the folder's name by default). Players drop the ZIP in their
Add-Ons folder (the Add-Ons screen's **Add-Ons Folder** button opens it)
and the game installs it as it is.

## The rest of the guide

Each topic has its own page:

3. [Game rules](rules.md), and [Rhai in ten minutes](rhai.md) if the
   scripting language is new to you
4. [HUD panels](hud.md)
5. [Weapons](weapons.md)
6. [Other content kinds](content-kinds.md)
7. [Old v20 Add-Ons and new bricks](old-addons.md)
8. [What players are asked to trust](trust.md)
9. [Tuning bodies in the game](tuning.md)
10. [Models from Blender (glTF)](gltf-models.md)
11. [Bodies that move their own way](../architecture/movement-controllers.md): movement controllers
12. [Client code: cameras, controls and effects](../architecture/client-sandbox.md)
13. [Experimental bot objectives](bot-objectives.md)

Porting an old Add-On's scripts is [porting.md](porting.md), and the
TorqueScript functions they used map to [torque-equivalents.md](torque-equivalents.md).

## Still being built

This guide changes in the same change as these land. Everything a total
conversion can and cannot change yet, area by area, is in
[total-conversion.md](../audits/total-conversion.md).

- **Brick authoring without v20 files**: a native brick format you write
  directly.
- **Drawing blocks**: `block` and `texture` content (per-face textures,
  flipbooks and states a script switches with `set_block_state`) load,
  save and replicate, but the renderer does not draw block faces yet.
- **Elevated client code**: joining asks "Trust and join" before a
  server's sandboxed client code runs ([What players are asked to trust](trust.md)), but code asking for
  `net.http` or `files.addon_folder` is not offered to joiners yet.
