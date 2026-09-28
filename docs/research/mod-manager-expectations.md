# What players expect from in-game mod management

Status: research, 2026-09-28. Feeds `docs/architecture/mod-manager.md`.
Sources are the games themselves as their players know them; nothing here is
copied from their code or art.

## The games people will compare us to

| Game | Where mods are managed | What players praise | What players complain about |
|---|---|---|---|
| **Blockland v20** | Start Game → Add-Ons: one checklist of every folder in `Add-Ons/`, with Default / All / None. Joining a server downloads its textures, sounds and models with a progress bar. | Zero setup: drop a zip in a folder, tick it, host. Joining "just works" for content. | No details, no search, no categories. A broken add-on only shows in the console. Server scripts and client content are the same thing to the player, so "why is this missing" is a mystery. Return to Blockland (RTB) became essential because it added an in-game browser, one-click install, updates and a mod manager. |
| **Garry's Mod** | Main menu → Addons: subscribed Workshop items with toggles, grouped by type (gamemode, map, weapon, tool, model), search, "disable all". Gamemode picker on the main menu. | Workshop subscribe → it is in the game. Servers download their Workshop content on the loading screen. | Missing content (purple checkers, ERROR models) with no explanation. Hundreds of addons, no way to tell which one broke something. Downloads you never asked for stay forever. |
| **Minecraft** | Vanilla: Resource Packs / Data Packs screens with Available and Selected columns, load order, "incompatible" labels; data packs chosen per world. Modded: launchers (CurseForge, Modrinth, Prism) with instances. | Per-world packs. Launchers resolve dependencies, check versions and keep separate profiles per modpack. | Version hell. Forge/Fabric mismatches. Joining a modded server means installing the exact same modpack by hand first. |
| **Factorio** | In-game Mods screen with the mod portal built in: search, install, update, enable, per-mod settings. | Dependencies enabled automatically. "Sync mods with save" makes your mod list match a save in one click. Joining a server downloads its mods. Clear error when a mod fails to load, naming the mod. Widely called the gold standard. | Very little. Startup-setting changes need a restart. |
| **Terraria (tModLoader)** | In-game Mod Browser and Mods list; Enable/Disable, Reload, mod packs (saved lists), per-mod config screens. | Mod packs as named, shareable lists. Config UI without editing files. | Reloading is slow; one crashing mod blocks the whole reload. |
| **Arma 3 / Space Engineers / Cities: Skylines** | Launcher or in-game Content Manager with presets, load order, and a server's required mod list. | Presets. The join screen lists the server's mods and downloads the missing ones. | Load order and conflicts are left to the player. |

## What players expect, in priority order

1. **Joining a modded server just works.** You click Join, it shows what the
   server needs, downloads it with a progress bar, and you are in. What you
   download for a server does not change your own setup. (Blockland v20 did
   this for content; Factorio and Garry's Mod do it for everything.)
2. **One screen, from the main menu, that shows what you have.** Toggle
   anything on or off. Search, and group by what it is (game modes, weapons,
   bricks, vehicles, looks). See a package's name, author, version and
   description without opening a folder.
3. **Errors in plain words, and a broken mod does not break the game.**
   "Creeper needs Creeper Model 1.x, which is not installed" beats a console
   stack. The rest of the list still loads. (Door-closer P0 "partial load
   with a report".)
4. **Dependencies take care of themselves.** Enabling something enables what
   it needs; disabling something that others need says so and offers to
   disable them too.
5. **Pick a game mode when starting a game.** Garry's Mod's gamemode picker,
   Minecraft's per-world data packs, Factorio's per-save mod list. Our
   packages can be game modes, so Start Game should offer them.
6. **Know what a mod is allowed to do.** Workshop trust is by popularity;
   ours can be exact because packages declare capabilities ("can change the
   world's bricks", "can hurt players"). Nobody else shows this well.
7. **Presets / profiles.** Named sets you can switch between and share
   (tModLoader mod packs, Arma presets, Minecraft instances).
8. **Get new mods without leaving the game.** Browse, install, update
   (Workshop, Factorio's portal, RTB). Needs a hosted service; later.
9. **Per-mod settings in a UI**, not config files (tModLoader, Factorio).
10. **Drop a zip in and it works.** Blockland players have folders of old
    add-on zips. Our importer (`bri-import-addon`) can turn one into a
    package; the manager should offer "Import Add-On…".

## Where we differ from Blockland v20

v20's add-on list was one flat checklist because every add-on was the same
kind of thing: a folder of TorqueScript and assets that the server executed.
Our packages are more than that (game modes, server-only rules scripts,
content kinds, client-only looks, per-server environments), so the manager
should keep v20's simplicity (one list, tick boxes, "Default") while showing
what each package is and where it runs:

- **Where it runs** matters to the player. A server-only package (rules,
  scripts) is never downloaded by players. A shared one must match to join.
  A client-only one (a HUD skin) is personal and never blocks a join.
- **What it provides** replaces v20's folder-name prefixes (`Weapon_`,
  `Vehicle_`, `Gamemode_`) as the grouping.
- **Server content is not your content.** Packages downloaded to join a
  server live in the download cache (PR #1), not in your list.

## What to build first

1. An **Add-Ons screen** from the main menu: every installed package with a
   tick box, grouped by what it provides, a details panel (name, version,
   author, where it runs, what it provides, what it needs, what it is allowed
   to do) and problems in plain words. Enabling pulls in dependencies;
   disabling asks before also disabling dependents. The base game is listed
   and locked.
2. A **join screen** that lists the server's packages you lack and downloads
   them with a progress bar, on top of PR #1's verified fetch.

Then, in order: Import Add-On (zip → package via `bri-import-addon`), game
mode choice in Start Game, presets, per-package settings, and a browser once
there is somewhere to browse.
