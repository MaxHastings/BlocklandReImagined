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
otherwise `partial`, with this copy's hash. To submit, copy `port/` to
`crates/addon-import/ports/<port>/` and put `submit.json` beside it as
`entry.json`, in a pull request or by handing the folder to someone who can.

Write each check from the v20 script, not from your port: it is the proof
that the port behaves like v20. A check with no numbers filled in fails.

## What a port is

A port lives in [`crates/addon-import/ports`](../../crates/addon-import/ports):

- `ports/<port>/entry.json` names the v20 Add-On the port covers and whether
  it is `verified` or `partial`. The list of ports is the folders there, so
  ports added on different branches never edit the same file.
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

An applied port also settles its datablocks: a function it covers, or an image
state script that calls one, becomes "ported by" that port, and a state script
the Add-On leaves to the stock `WeaponImage` (`onFire`, `onCharge` and the
others `WeaponsWorld::NATIVE_STATE_SCRIPTS` lists) "runs the engine's own".
A datablock with nothing else outstanding is `converted`. For a copy listed by
its hash, a script global it sets at load that a covered function reads (the
Grapple Rope's `$Pref::Server::GrappleRopeAnywhere`) is noted as ported with
that value. A field write that runs only when a required Add-On was turned off
(`if (%error == $Error::AddOn_Disabled)`, hiding its item) is a note, not a
gap: turning a package on turns what it needs on with it.

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
| `covers` | Each function the port replaces, with named patterns (regular expressions, case-insensitive) its body must match. The first group of each is a value the port can use. A key that is one of the Add-On's script files (`server.cs`, `server/core/Slayer_MiniGameSO.cs`) matches that file's whole text instead, for values it sets outside any function, such as a table of globals or a preferences file. |
| `tests` | the port's own checks (`<port>/checks.json`, which `check-port` runs) and any `path test_name` in the repository. The list's own test checks that they exist. |

Patterns do two jobs. They prove the copy is the shape the port was written
for, and they read the numbers from that copy's script, so a port never
hard-codes one copy's values. In a patch, a string that is exactly
`"{projectiles}"` becomes the captured value (a number when it reads as one).
`{name}` inside a longer string becomes its text, and `{name:lower}` its
text in lower case, for ids: Torque ignores the case of names
(`"weapon_example:projectile/{jab:lower}"`). A string that is exactly
`"{name:rgba}"` becomes a colour as Torque writes one (`"0.6 0.7 0.4 1"`
becomes `[0.6, 0.7, 0.4, 1]`).

One addition to RFC 7396: an object patching a list whose items all have an
`id` patches the items its keys name (and adds any it names that are not
there), so a port reaches one kind in `bots.json`
(`{"bots": {"{namespace}:bot/zombieholebot": {"emote": "hug"}}}`) as it
reaches one image in `weapons.json`.

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

### What the port carries out

A port also names what it carries out without a rules function of the
same name, in `handles`: each key is an original function, `call:<name>`
(a top-level call at load), `new:<Class>` (an object made at load),
`set:<global>` (a top-level assignment), `file:<path>` (a file the
importer does not convert), `datablock:<name>` (a datablock it does not
convert) or `pref:<global>` (an RTB preference the game carries out with
no setting, a bug fix the engine always makes), any case, and its value says how, in a sentence a reader can
check. The report lists these under "Carried out by the port" and counts
them as ported, and such a datablock as `ported`. Name only what the port
does; a behaviour it does in part stays unported until it does all of it,
and something deliberately not run says so and why ("not run: ...").

```json
"handles": {
  "ND_SelectionBox::setSize": "the rules' draw_selection_box: show_shapes sizes the faces, edges, corners and label to the box",
  "datablock:ND_SelectionBoxBorder": "drawn with show_shapes: the twelve edges, as wide as the box is big",
  "servercmdLight": "the magazine's light_states: the light key reloads in Ready, Empty and EmptyFire, else works the light"
}
```

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
   | Anything a field in [Weapons](weapons.md) or [Other content kinds](content-kinds.md) expresses | a patch setting that field |
   | An image's state script (`onCharge`, `onFire`, a custom `stateScript` such as `onFiretwo`) that plays an arm animation, calls `Parent::onFire`, spawns a second projectile or uses the item up | an entry in the image's `scripts` ([torque-equivalents.md](torque-equivalents.md#image-state-scripts-as-data)) |
   | Something the game already does the same way | nothing: cover the function with patterns and say so in `notes` |
   | A `serverCmd` in an Add-On with no weapons, vehicles or bricks | a rule ([Game rules](rules.md)): `behaviour.json` and a script under `files/`, and a `package.json` patch adding them to `provides` and their `capabilities` |
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

7. **List it** in its folder's `entry.json` with its status and tests, then run
   `cargo test -p bri-addon-import`.

### Datablocks made at run time

Some Add-Ons make datablocks in a function or a loop (Slayer CTF's
`createSlayerCTFDatablocks` makes a flag item and image for each of ten
paint colours), so the importer, which reads scripts without running them,
finds none. The port declares them in `ports/<port>/datablocks.cs`, written
the way the Add-On would have:

```
datablock ItemData(slyrCTF_FlagItem)
{
	shapeFile = "{{flag_shape}}";
	uiName = "{{flag_name}}";
	image = slyrCTF_FlagImage;
};
```

The importer reads it beside the Add-On's own scripts, in its folder, before
converting anything, so its paths, parents and globals resolve as the
Add-On's do (`mountPoint = $BackSlot` reads the base game's value).
`{{name}}` takes the values the `covers` patterns captured, and
`{{namespace}}`. The report notes how many datablocks it declared. The
declarations are the port's own text, never the Add-On's.

Sounds declared this way (Slayer's countdown voices, made in a loop) play
by id from the rules, `play_sound(p, "<namespace>:sound/<name>")`: every
converted `AudioProfile` goes into the Add-On's weapons pack, which an
Add-On with no weapons gets just for its sounds.

A `covers` pattern reads a function's plain definition, the last one if
the Add-On defines it twice, as Torque keeps. A definition inside a
`package` wraps that one (it calls `Parent::`), so it is read only when
there is no plain one.

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
| `Weapon_Package_Tier1` (Kai's Tier+Tactical Tier 1) | `weapon_package_tier1` on `_shared/tier-tactical-core` and `_shared/tier-tactical` | partial | the Tier+Tactical ammo system: magazines run by each image's check states, the T+T2 reserves, the light key's reload, ammo items and a dead player's ammo bag; raycast pistols reaching less on the move, the pump's pellets and blast loaded a shell at a time, the sport rifle's weak round on the move and its headshots under their own kill message, the submachine gun slowing whoever it hits, the akimbo pistols' left hand, recoil kick. Every preference it registers but four is a server setting: its starting and most ammo, bots' endless ammo, endless grenades, what the dead drop and ammo pickups (`rules.prefs`); the ammo system, ammo display and its time, recoil, bullet slowing, always reloading, grenade count display, remounting a second copy, clearing spent grenades, the easter eggs and leaving out Tier 1 or its ammo boxes (`rules.settings`, the last three from the next start or map). Its four bug-fix preferences (firing and reload sounds after death, a slot's rounds after SetInventory or RemoveItem events) have no setting: the engine always behaves as with the fix, and the report says so, though Kai left RemoveItem Bugfix off. Not yet: the ammo items' floating count, the recoil shake for players nearby |
| `Weapon_Package_Tier1A` (Kai's Tier+Tactical Tier 1A) | `weapon_package_tier1a` on `_shared/tier-tactical-core` and `_shared/tier-tactical` | partial | on Tier 1: the single shotgun's pellets, close blast and knockback, the pepperbox's several rays a shot, the snubnose's headshots, the nailgun's slowing nails. The nailgun is an easter egg the original loads only with its hidden `???` setting, off by default: it is imported hidden and offered while Tier 1's `???` setting is on, from the next start |
| `Weapon_Package_Tier2` (Kai's Tier+Tactical Tier 2) | `weapon_package_tier2` on `_shared/tier-tactical-core` and `_shared/tier-tactical` | partial | on Tier 1: the assault rifle's truer round after a pause (its projectile when rested), the light machine gun's free second round each cycle (`state_shots`) and its slowed laid-down body while firing (its `PlayerData` as an archetype, pushed and popped by host rules), the combat shotgun's jet-press blast mode (an alt image of two shells' pellets and blasts that hands back), the battle rifle's and machine gun's slowing rounds, the magnum's headshots, and the military sniper's hitscan crits under Emote_Critical (×3, the crit's kill message and effects, only while Emote_Critical is on). The scoped magnum is an easter egg behind the hidden `???` setting: it is imported hidden and offered while that setting is on, from the next start |
| `Weapon_Package_Tier2A` (Kai's Tier+Tactical Tier 2A) | `weapon_package_tier2a` on `_shared/tier-tactical-core` and `_shared/tier-tactical` | partial | on Tier 1: the bullpup's three-round burst (its check states, each round spent as it loads), the dual SMGs' free left gun (`left_image`, `onFireAkimbo`), the MachStil's hitscan, the scoped carbine's jet-press scope (a scoped image that slows the holder while held and hands back on reload: `commands.mount`, `unmount`, `states`) and its forced reload that a check keeps going (`keeps_reload`). The match pistol is an easter egg behind the hidden `???` setting: it is imported hidden and offered while that setting is on, from the next start |
| `Weapon_Package_Explosive1` (Kai's Tier+Tactical Explosive 1) | `weapon_package_explosive1` on `_shared/tier-tactical-core` and `_shared/tier-tactical` | partial | on Tier 1: grenades counted from the reserve (`from_reserve`: put away with the last, back in hand as a grenade bag brings more), the conc's knock at each bounce and its burst on the second, the firebomb's arm raised once while it waits, its two or three embers thrown as its script threw them (`steps`), each searing the players near it four times with flames and a sizzle only they hear (`aura`). The toss sound plays once where the original played it twice. Every preference it registers is a server setting, among them grenade drop, endless grenades, starting and most grenades, leaving out the pack or its grenade bags (from the next start), the molotov's targeting fix (`aura` `max_targets`) and its friendly fire override, under which its fire sears the thrower's teammates for 1 a time (`aura` `ally_damage`) |
| `Weapon_Package_Explosive2` (Kai's Tier+Tactical Explosive 2) | `weapon_package_explosive2` on `_shared/tier-tactical-core` and `_shared/tier-tactical` | partial | on Tier 1: the RPG a little wild fired on the move, the grenade launcher, the Calibre Cannon's flak round shedding sparks every loop as it flies and bursting three to five more at whatever it hits (`children` `angles`, `on_hit`, `redraw`), the reload clicks as the base game's. The mortar is an easter egg behind the hidden `???` setting: it is imported hidden and offered while that setting is on, from the next start; it lobs its shell by how far its holder's look lands (`shot.lob`). Tier 1's preferences reach its guns |
| `Weapon_Package_Medic1` (Kai's Tier+Tactical Medic 1) | `weapon_package_medic1` | partial | the gauze gun's dart heals at once and then over time through the emote slot (`medigunHealImage`'s looping `onHeal`), a new hit restarting it; the stimpack booster heals its holder with a cooldown, a charging message and a recharge notice, and its jet throws the syringe. Who can be healed follows `TT_canHeal` (same minigame, only the healer's team or an allied one in a game with teams, LAN host outside one, own bots), and its two preferences (Can Heal Bots, Teams Can Heal Enemies) are server settings |
| `Weapon_Melee_Extended` (Kai's Tier+Tactical Melee Extended) | `weapon_melee_extended` on `_shared/tier-tactical-core` and `_shared/tier-tactical-melee` | partial | eleven melee weapons hitting along a short ray from the eye, each swing drawing one of its pair of hit sounds (hitscan `sounds`), the arm and other-arm moves its scripts played (`arm`, `gesture`), the combat knife's quick-click stab and charged slash as two fire states with their own damage (`state_shots`, hitscan `damage`, held to the raycast script's 100). Hits never crit, as its crit test looks for zombies |
| `Weapon_Melee_Extended_II` (Kai's Tier+Tactical Melee Extended II) | `weapon_melee_extended_ii` on `_shared/tier-tactical-core` and `_shared/tier-tactical-melee` | partial | five more melee weapons and the chainsaw (its revving arm cue played on mount), plus the riot shield: raised, it covers what its holder faces (Kai's up/down limits on where a hit lands), keeps a tenth of a shot and a quarter of other harm, sends shots back as the holder's own (kill text "Reflected", a special kill), plays its clang and breaks after twenty stops. The three easter-egg weapons import hidden and are offered while Tier 1's `???` setting is on. Its three shield preferences are server settings bound to the guard: durability (-1 never breaks), whether bots' shields wear out (`bots_keep`) and whether a raised shield takes all but an eighth of a fall it faces (`fall_damage`) |
| `Weapon_Skins_Pistol` (Kai's Tier+Tactical Pistol Skins) | `weapon_skins_pistol` on `_shared/tier-tactical-core` and `_shared/tier-tactical` | partial | each skin is Tier 1's pistol under its own name, model and numbers; its scripts copy the host's, so the shared rules port them |
| `Weapon_Skins_Dualies` (Kai's Tier+Tactical Dualies Skins) | `weapon_skins_dualies` on `_shared/tier-tactical-core` and `_shared/tier-tactical` | partial | each skin is Tier 1's Akimbo Pistol, each holding twice its Pistol skin's magazine (a `TT_maxAmmo` field written as `classicPistolItem.TT_maxAmmo*2`, evaluated as the datablock loaded) under its own name, model and numbers; its scripts copy the host's, so the shared rules port them |
| `Weapon_Skins_Rifles` (Kai's Tier+Tactical Rifle Skins) | `weapon_skins_rifles` on `_shared/tier-tactical-core` and `_shared/tier-tactical` | partial | each skin is Tier 1's Sport Rifle; the Bolt Rifle shifts its arm as it is drawn (the shared onMount cue rule) under its own name, model and numbers; its scripts copy the host's, so the shared rules port them |
| `Weapon_Skins_SMG` (Kai's Tier+Tactical SMG Skins) | `weapon_skins_smg` on `_shared/tier-tactical-core` and `_shared/tier-tactical` | partial | each skin is Tier 1's Submachine Gun under its own name, model and numbers; its scripts copy the host's, so the shared rules port them |
| `Weapon_Skins_Shotgun` (Kai's Tier+Tactical Shotgun Skins) | `weapon_skins_shotgun` on `_shared/tier-tactical-core` and `_shared/tier-tactical` | partial | each skin is Tier 1's Pump Shotgun under its own name, model and numbers; its scripts copy the host's, so the shared rules port them. The Classic Shotgun is an easter egg behind the hidden `???` setting: imported hidden, offered while that setting is on |
| `Weapon_Skins_LMG` (Kai's Tier+Tactical LMG Skins) | `weapon_skins_lmg` on `_shared/tier-tactical-core` and `_shared/tier-tactical` | partial | each skin is Tier 2's Light MG, laying and lifting its gunner's slow body under its own name, model and numbers; its scripts copy the host's, so the shared rules port them |
| `Weapon_Skins_Magnum` (Kai's Tier+Tactical Magnum Skins) | `weapon_skins_magnum` on `_shared/tier-tactical-core` and `_shared/tier-tactical` | partial | each skin is Tier 2's Magnum under its own name, model and numbers; its scripts copy the host's, so the shared rules port them. The Retro Magnum's onReloadWait is named by none of its states, so v20 never ran it. The Retro Magnum is an easter egg behind the hidden `???` setting: imported hidden, offered while that setting is on |
| `Weapon_Skins_RiflesT2` (Kai's Tier+Tactical Tier 2 Rifle Skins) | `weapon_skins_riflest2` on `_shared/tier-tactical-core` and `_shared/tier-tactical` | partial | each skin is Tier 2's Assault and Battle Rifles under its own name, model and numbers; its scripts copy the host's, so the shared rules port them. The Browning, Scout, Semi-auto Battle and Compact rifles are easter eggs behind the hidden `???` setting: imported hidden, offered while that setting is on |
| `Weapon_Skins_ShotgunT2` (Kai's Tier+Tactical Tier 2 Shotgun Skins) | `weapon_skins_shotgunt2` on `_shared/tier-tactical-core` and `_shared/tier-tactical` | partial | each skin is Tier 2's Combat Shotgun, without the double blast, as the skins define no onAltTrigger under its own name, model and numbers; its scripts copy the host's, so the shared rules port them |
| `Weapon_Skins_Sniper` (Kai's Tier+Tactical Sniper Skins) | `weapon_skins_sniper` on `_shared/tier-tactical-core` and `_shared/tier-tactical` | partial | each skin is Tier 2's Military Sniper and its crits under its own name, model and numbers; its scripts copy the host's, so the shared rules port them |
| `Weapon_Skins_Bullpup` (Kai's Tier+Tactical Bullpup Skins) | `weapon_skins_bullpup` on `_shared/tier-tactical-core` and `_shared/tier-tactical` | partial | each skin is Tier 2A's Bullpup and its burst under its own name, model and numbers; its scripts copy the host's, so the shared rules port them |
| `Weapon_Skins_MPistol` (Kai's Tier+Tactical Machine Pistol Skins) | `weapon_skins_mpistol` on `_shared/tier-tactical-core` and `_shared/tier-tactical` | partial | each skin is Tier 2A's Machine Pistol under its own name, model and numbers; its scripts copy the host's, so the shared rules port them |
| `Weapon_Impact_Rifle` (Kai's Impact Rifle) | `weapon_impact_rifle` on `_shared/tier-tactical-core` and `_shared/tier-tactical` | partial | a bolt-action round with its own blast on Tier 1's ammo system, spreading a little when its holder stands still and none on the move, as its check (still, and a pause since a last shot only Tier 2's Assault Rifle and Light MG note) reads |
| `Weapon_ShortRifleKai` (Kai's Short Rifle) | `weapon_shortriflekai` on `_shared/tier-tactical-core` and `_shared/tier-tactical` | partial | a ray on Tier 1's ammo system that ricochets as its fireRaycast does (the hitscan `ricochet`: its raycastRicochets turns, more damage for each landing before, its share on its own shooter), shoves whoever it hits, and with Emote_Critical on crits a hit made after a turn (from below, else to the head) under its own crit kill message |
| `Gamemode_Slayer` (Slayer 4.1.5) | `gamemode_slayer` | partial | Game modes (Deathmatch, Team Deathmatch and modes other Add-Ons add), lives, points and time to win, rounds and resets, the pre-round countdown on `PlayerFrozenArmor`, teams that sort, balance and spawn on team spawns, `/teams` and its short forms, friendly fire and team chat, capture points (trigger zones, brick events), spectating out of lives (orbit, free and auto cameras), the fly-through camera (`/createFlyCam`, `/setKnot`, `/setJump`, `/testFlyCam`) and Slayer's event outputs (`setTeamControl`, `setTeamControlLocked`, lives, kills and deaths, `joinTeam`, round time, `Win`, `checkTeam`, `checkTeamCount`, `StartFlyThrough`) with the `Team(Client)` and `Team(Brick)` targets and their outputs (`ChatMsgAll`, `CenterPrintAll`, `BottomPrintAll`, `RespawnAll`, `IncScore`), the `onPlayerTouch(TeamN)`, `onActivate(TeamN)` and `onMinigame` inputs, and Restrict Output Events, all as host rules. Not yet: uniforms, team loadouts and player types, bots, saved fly-through paths |
| `Gamemode_Slayer_CTF` (Slayer CTF) | `gamemode_slayer_ctf` | partial | Capture the Flag: flags on Flag Spawns in their brick's colour, pickup, carrying on the back, capture, recovery, dropping (death, leaving, `/dropFlag`, the `DropFlag` event output), respawn timers, captures to win, the CTF preferences (`/ctf`), its brick event inputs. Not yet: the Drop Tool key, the countdown over a dropped flag, the flag's light, locked flags, score list columns, bots, the other flag models |
| `Tool_GrappleRope` (Grapple Rope) | `tool_grapplerope` | verified | host rules: where the hook strikes with a clear line of sight from `lift` above the feet, the holder hangs on a rope (`tether`) as long as the distance then while the click is held, and flies off with their speed on letting go; the image draws the rope with the chain projectile's trail (`rope`). The engine's rope stands in for `GrappleRope`'s 10 ms velocity correction; the movement keys steer only by the player's air control, as in v20 |
| `Weapon_Loz_Hookshot` (Hookshot) | `weapon_loz_hookshot` | verified | host rules: where the spearhead strikes, the shooter's speed is set straight at the spot every `every` ms, `fast` beyond `far` and `slow` within `near`, until within `stop`; a struck player or vehicle is followed; a seated shooter pulls their vehicle only toward a player or vehicle; `/degrapple` stops it. All numbers read from the copy |
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

**Player types.** An Add-On's `PlayerData` becomes an archetype in the
import itself, `assets/archetypes/<name>.json` as `<ns>:archetype/<name>`:
a player type is data, so a shared package may carry it, and only the
host's archetype table counts. Its fields and its ancestors' in the Add-On
(the nearest wins) lie over the archetype of the first one outside it: a
player type of an Add-On it depends on, or one of v20's. Fields no
archetype field carries are listed in the report.

**Names.** In rules files, `{{name}}` becomes a value at import:
`{{namespace}}` (the import's id), `{{rules}}` (the rules' id),
`{{version}}`, or anything a `covers` pattern captured. A `{{word}}` that
names nothing is an error. The importer's ids are
`<ns>:<kind>/<datablock name in lower case>`, so a rule gives out the
imported item as `"{{namespace}}:weapon/fillcanitem"`. `{{name|bool}}`
writes a captured TorqueScript truth value (`1`, `0`, `true`, `false`) as
`true` or `false`, for a setting's `default` in `behaviour.json`, and
`{{name|lower}}` writes it in lower case, as content ids spell a Torque
name (`"v20.weapon.{{team_equip_0|lower}}"` for a captured `hammerItem`):

```json
{ "key": "auto_sort", "title": "Auto Sort", "type": "bool", "default": {{pref_auto_sort|bool}} }
```

**Rules that build on another Add-On's rules.** An Add-On written for
another (a Slayer game mode) reads that one's settings or adds to its lists.
`"needs": { "slayer_rules": "Gamemode_Slayer" }` in `rules` makes the rules
depend on that Add-On's rules and gives their id as `{{slayer_rules}}`, as
the importer names them from the Add-On's folder name: Slayer CTF reads
`setting(game, "{{slayer_rules}}:mode")` and adds Capture the Flag to the
mode list with `setting_items`.

**Preferences become settings.** A preference the original's GUI edited
(Slayer's `Slayer_PrefSO`, `$Pref::` values an Add-On menu changed) is a
`settings` entry in the rules' `behaviour.json`, its default captured from
the original by a `covers` pattern, so the host edits it in the Mini-Game
window's Add-On Settings and the rule reads it with `setting(game, key)`.
Preferences that duplicate the vanilla mini-game dialog (damage, building,
points per kill, respawn times, starting equipment) stay in that dialog.

**RTB server preferences.** The importer reads every
`RTB_registerPref(title, category, global, type, add-on, default, restart,
hostOnly)` call of the copy (`bool`, `int min max`, `list Name value ...`
and `string length` types; the default may be a product such as `35*4`).
Those the rules read become server-wide settings of the rules on their own:
list their globals in `rules.prefs`, to how the rules use them (a trailing
`*` matches the rest, `"$Pref::Server::TT::Start*"`), and read each with
`pref("$Pref::Server::TT::Start9MM")`, which finds it whichever running
Add-On registered it (Tier 1's ammo bag reads Explosive 1's grenade drop
preference). The report counts them as ported; the copy's other preferences
stay unsupported, saying the default they keep. One that needs a restart
(read while the Add-On loads) becomes a restart setting: the host's change
applies when the server starts again or loads a map. The game plays the copy's
RTB branch, so a `TT_defaultIfUnset`-style fallback for servers without
RTB never runs, and `isFunction(registerPreferenceAddon)` (Blockland Glass)
reads as absent.

**Weapon fields from settings.** A preference that changed what a weapon
does where its scripts read it (Tier's Recoil around the recoil blast, its
Ammo System in every ammo function) goes in `rules.settings` instead, by its
global, with the fields it sets: the importer writes them into the import's
pack as `bindings` ([Weapons](weapons.md), "Fields from
server settings"), so the weapons follow the host's setting.

```json
"settings": {
  "$Pref::Server::TT::Recoil": {
    "how": "off, the guns lose their recoil kick",
    "fields": [{ "path": ["images", "*", "shot", "kick"], "values": { "false": null } }]
  },
  "$Pref::Server::TT::Ammo": {
    "how": "the guns' magazines follow it",
    "fields": [
      { "path": ["images", "*", "magazine", "supply"], "existing": false,
        "only": { "item.TT_reloads": true },
        "values": { "0": "reserve", "1": "endless", "2": "unlimited", "3": "counted" } },
      { "path": ["images", "*", "magazine", "supply"], "existing": false,
        "only": { "item.TT_alwaysReloadPref": "Ex" },
        "when": { "$Pref::Server::TT::AlwaysReloadEx": "true" },
        "values": { "2": "endless", "3": "both" } }
    ]
  }
}
```

`*` is every id of that kind the import declares (or every key there).
The field must already be in the pack unless `"existing": false` (a field
left at its default); every step before it must be. `only` keeps the
definitions whose datablock has those fields (`true` set, `false` unset, a
value with `*` at its start or end matching the rest, or a list of any of
these; `item.<field>` reads an image's item and `datablock` is the
definition's own datablock name: `{ "datablock": "*staticItem" }`).
`values`, `scale` and `when` are the binding's; with both, a listed value
wins and the others are scaled. The bindings name the global, so a pack
whose copy does not register the preference (Tier 2) binds its guns to the
one another Add-On registers (Tier 1). The report counts the preference as
ported with its `how`.

A preference RTB applied only at the next start (its restart argument)
becomes a setting that waits for the next start or map, so a setting that
leaves items out (`["items", "*", "hidden"]`, Tier's Disable Tier 1 and
easter eggs) takes them out of every item list then. Rules that only
declare settings need a `rules/behaviour.json` and a script that does
nothing, and no capabilities (Melee Extended II).

**Reaching the rules.** In the patch, `{namespace}`, `{rules}` and
`{version}` work like captured values, in keys too. Point the image's
moments at the rules' commands ([Weapons](weapons.md),
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

A rule hears its import's projectiles with `"on_projectile_hit": true` in
`behaviour.json`, the native form of `<projectile>::onCollision`; the
shooter is the caller, so a hit may `paint_fill` or `paint_vehicle` for
them. The ported
Fill Can (`ports/tool_fill_can`) is the worked example: its image keeps
firing its own projectile, `paint_picker` keeps it out when a can is
picked, and its rules fill or paint what the shot hit.

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
| `every` | magazine fields for every gun (`"light_states": ["Ready", "Empty"]`, `"display_ticks": 480` for an ammo display that stays up four seconds each change, `"display_scripts": ["TT_onEmptyFire"]` for states that show it again), before `items` |
| `items` | extra magazine fields for one item, by datablock name |
| `calls` | the ammo system's own functions (`hl2AmmoCheck`): a state script may call them and still count as only working its rounds, and each one the copy defines is reported as carried out by the magazine |
| `scripts` | more image methods checked the same way (`onMount`, `onReload`); one that only works its rounds is reported as read |

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

A table can instead hold the calls a copy makes outside any function, one
row per call: `"call": "TT_registerAmmoType"` with `fields` naming each
argument in order (`""` skips one) and `key` one of those names. A pack's
rules can then hand out exactly the ammo types its copy registers.

## Script rules

A family of Add-Ons often writes the same few lines in every gun's
methods: `%this.TT_raycastSpreadAmt = 0.002;` on the move,
`%obj.mountImage(LeftImage, 1);` in `onMount`, `TT_dampenVelocity(%col, 2);`
in a projectile's `damage`. `"scripts"` reads them from each copy's own
bodies, in order, a later rule's fields winning:

```json
"scripts": [
  { "method": "onMount", "into": "image",
    "pattern": "%obj\\.mountImage\\(\\s*(?P<i>\\w+)\\s*,\\s*1\\s*\\)",
    "set": { "left_image": "{i|image}" } },
  { "on": "projectile", "method": "damage", "into": "projectile",
    "pattern": "TT_dampenVelocity\\(\\s*%col\\s*,\\s*(?P<d>[\\d.]+)\\s*\\)",
    "set": { "slow": { "divisor": "{d}" } } }
]
```

| Field | Meaning |
|---|---|
| `on` | `image` (the default), `projectile` or `item`: whose methods |
| `method` | the method (`onFire`, `damage`), several as `onFire\|onFire2`, or `*` for every state script of an image, also with others (`*\|onMount` for a method no state runs) |
| `into` | for an image: `image`, `shot` (the shot the method fires: `onFire`'s is the image's `shot`, another state script's is its `state_shots` entry), `magazine`, `check` (the magazine's `checks` entry for that script, as the states spell it: a check that `spend`s a round as it loads, or `keeps_reload`) or `state` (each state running the method); for a projectile: `projectile`; for an item: `item` (an ammo box's `label`, `rotate` for `%obj.rotate = true`). Any can fill a `table` instead |
| `table` | with `into: "table"`: the rules' `{{name}}`, a Rhai map from each image's or projectile's id to its `set` |
| `pattern` | a case-insensitive regex; its named groups fill `set` |
| `required_by` | when a body matches this but not `pattern`, the port stops and names the image, so a copy that does the same some other way is not guessed |
| `set` | a merge patch: `"{group}"` becomes the group's value (a number when it reads as one; `"{group\|text}"` keeps it text), `"{group\|field}"` the value of the datablock field the group names (`%obj.TT_ammoPickup[0]`), `"{group\|word1}"` its second word (`getWord`), filters in that order, `"{group\|seconds}"` milliseconds as seconds, `"{group\|neg}"` that number negated (a push the script wrote as negative), `"{group\|projectile}"`, `"{group\|image}"` and `"{group\|sound}"` the import's datablock it names, `"{group\|kick}"` the camera shake of the explosion a projectile names (this Add-On's, or one it depends on); a `null` removes a field |
| `keep` | with `state`: only fields the state leaves empty |

Every reader here follows a datablock's parents into the Add-Ons it
depends on, as v20 did: an item `datablock SkinPistolItem(x : PistolItem)`
gets the magazine fields Tier 1's `PistolItem` gave it, when the import
has the drop folder (or `--reference`) to read Tier 1 from. A table row
whose archetype filter names a player type the import cannot find is left
out rather than stopping the port.

An image firing a projectile of an Add-On it depends on (Tier 2's sniper,
Tier 1's tracer) names it by the id that Add-On's own package gives it:
the base game's `v20.projectile.<name>` for a stock one, else
`<its namespace>:projectile/<name>`. The pack lists it in
`external_projectiles` instead of carrying a copy, and the game resolves
it when it loads the packs together; without the dependency the image is
left out. The readers above see those projectiles beside the import's own.

The importer follows `exec` from `server.cs`. When every `exec` in the
scripts it reaches names a plain path, a script none of them reaches is
left out, as v20 never ran it (Tier 2A's unused `Weapon_Unused.cs`); the
report says so.

## Shared parts

Add-Ons built on one support script (Tier+Tactical's ammo system, used by
26 packs) share their port: `ports/_shared/<name>.json` holds any of
`port.json`'s fields, and `"include": ["<name>"]` in a port applies it
first. The port's own fields merge over it and its `scripts` follow the
shared ones. Host rules too: `ports/_shared/<name>/rules/` comes before
the port's own `rules/`: a script of the port's replaces the shared one
of the same name, and its `behaviour.json` merges over the shared one (a
port adds its commands or hooks; a list such as `commands` replaces the
shared list).

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

Raycasting support scripts gave each image fields for its ray: Space
Guy's `raycast*`, and the copies of it such as Tier+Tactical's
`TT_raycast*`. `"hitscans"`
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
| `hit_projectile` | the field naming the projectile exploded where a ray lands (nothing when an image leaves it empty); without it, the image's projectile's own explosion |
| `impulse`, `vertical` | the shove along the shot and straight up |
| `count`, `spread`, `spread_degrees` | rays per shot and their spread (in degrees across with `spread_degrees`) |
| `tracer` | `{ "field": ..., "look": { "color", "width", "seconds" } }`: a streak for images where the field is set |
| `flown` | the field naming a projectile flown from the muzzle to where the ray ended, as the script spawned it |
| `player_sound`, `other_sound` | the fields naming the sounds where a ray lands on a player, and on anything else |
| `eye_within` | with a muzzle cast, the distance in front of the eye within which anything makes the ray start at the eye (a script's obstruction check) |
| `converge` | with a muzzle cast, aim at the point the eye looks at (`getLOSPoint`) |

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
server.cs")`, as ModernWarbattles did), list it in `"loads"` instead: the
rules then name it in `optional_dependencies` and in `companions`, so
turning the Add-On on turns that import on too when it is installed, the
Add-On runs without it when it is not (the `exec` of a missing file did
nothing), and `{uses:...}` names it the same way.

A `handles` key `dependency:<original Add-On name>` can document a source
framework replaced by native code. Pair it with a `package.json` patch that
removes that framework's runtime dependency. Only a successfully applied
port whose manifest no longer requires it marks the source dependency
`ported`; its original name, call and location remain in the report. Removing
a manifest requirement alone does not hide an unresolved source dependency.
