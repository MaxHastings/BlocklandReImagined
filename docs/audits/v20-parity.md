# v20 parity and beyond-v20 audit

Audited 2026-09-28 against main `1932015`, branch `claude/v20-parity`.
Part 1 lists every player-facing Blockland v20 feature and marks it against
our game. Part 2 rates the layer we add on top of v20: quality of life,
modding, documentation and setup. The fix log at the end records what this
branch changed.

## How this was checked

- **v20 side.** The recovered v20 GUIs and scripts under `.research/v20-dso/`
  (`client/ui/allClientGuis-Vanilla.gui`, `client/scripts/allClientScripts-Vanilla.cs`,
  `server/scripts/allGameScripts-Vanilla.cs`) and the reference install
  `E:\Downloads\B4v21Launcher\versions\Blockland v20`. Every top-level GUI,
  every control with a `command` or `variable`, the 81-entry `$RemapName`
  key list, every `moveMap`/`GlobalActionMap` bind, all 88 `serverCmd*`
  functions and every `registerInputEvent`/`registerOutputEvent`.
- **Our side.** Our screens run v20's own converted layouts and dispatch each
  authored `command` string through an exact allowlist. Anything not on the
  list answers "Interface under construction". The new test
  `crates/ui/tests/authored_buttons.rs` clicks every visible, active authored
  button on 19 menu screens against the real converted pack (`ui-pack-003`)
  and fails on that answer. On `1932015` it found 11 such buttons (listed
  below). Its ignored `list_inert_buttons` companion lists buttons whose
  click changes nothing; most are empty lists in the probe, and the real
  gaps it surfaced are named below.
- Line numbers are from `1932015`.

## Labels

- **Present**: a player gets what v20 gave.
- **Partial**: works, but a visible piece of the v20 behaviour is missing.
- **Missing**: a v20 player looks for it and it is not there, or the button
  says "Interface under construction".
- **Lane**: owned by another thread; recorded, not touched here.
- **Dropped**: deliberately not carried over (master server, Blockland
  account keys, 2009 driver settings). Not counted as missing.

## Summary

| | Present | Partial | Missing | Lane | Dropped |
|---|---|---|---|---|---|
| Part 1 (v20 features, 120 rows) | 74 | 5 | 27 | 7 | 7 |

The misses a player notices first, most noticeable first:

1. **Main menu Credits button** says "Interface under construction", and
   **F1 help does nothing**. v20 opened HelpDlg with eight help pages
   (credits, controls, getting into the game, building, brick appearance,
   destroying bricks, saving, loading).
2. **Start Game > Advanced Config** says "Interface under construction", and
   **none of v20's server settings are applied**: max chat length,
   bricks per second, too-far build distance, random brick colour, the
   chat filter, falling damage, vehicle limits and the per-player quotas.
   Admin > Host Options is hidden because the host has no adapter for it
   (`crates/sim/src/session/admin.rs:601`).
3. **Join Server column headers** (name, players, ping and so on) and the
   **Filters** button say "Interface under construction".
4. **Start Game > Music Files** says "Interface under construction".
5. **Options hides settings v20 players reach for**: vehicle mouse invert
   (the game already reads it, `crates/client/src/app.rs:4033`), Censor
   Chat, Press Up to Repeat Chat, the temp brick paint-colour toggles, Auto
   Light, steering auto-return, and Render My Player/Items/Jets.
6. **`/brickcount`** (any player) and **`/spy` / `/ret`** (admins) are
   unknown commands.
7. **Mini-game favourites**: Create Mini-Game's ten preset slots and Set
   Favs do nothing.
8. **Load Bricks colour-set mismatch** (append, replace or match colours)
   has no screen.

Part 2 in one line: the modern layer is **solid** on hosting, Add-On
packaging, the sandbox and safety, **rough** on documentation drift and on
the modder path for client code and bricks, and **missing** a native brick
format and in-game help. Details are in Part 2.

---

# Part 1: v20 features

### Main menu (`MainMenuGui`)

| Feature | Status | Evidence |
|---|---|---|
| Start Game | Present | `crates/ui/src/screens/menus.rs:710` |
| Join Game | Present | `menus.rs:710` |
| Player (avatar) | Present | `menus.rs:716` |
| Options | Present | `menus.rs:711` |
| About | Present | `menus.rs:717`, text at `menus.rs:111` |
| Credits (`getHelp("1. Credits")`) | Missing | `authored_buttons` test: "Interface under construction" |
| Tutorial | Present | `menus.rs:719`; see `docs/audits/night-qa.md` finding B for LAN |
| Quit | Present | `menus.rs:698-706` |
| Version line | Present | `menus.rs:27` |
| Account key, name, store, demo mode | Dropped | hidden, `menus.rs:73` |

### Options (`optionsDlg`)

Tabs: Graphics, Advanced, Audio, Controls. Network is hidden on purpose
(`crates/ui/src/screens/options.rs:735`); its Download Textures/Sounds/Music
choices belong to the Add-Ons download pipeline (Lane: night QA).

| Feature | Status | Evidence |
|---|---|---|
| Resolution, fullscreen, VSync, Apply | Present | `options.rs` (`FULLSCREEN`, `NO_VSYNC`, `RESOLUTION`) |
| Shadow quality | Present | `options.rs` (`SHADOW_QUALITY`), `client/src/quality.rs` |
| Lighting, particle, texture, physics, brick FX quality radios | Partial | hidden; replaced by the Graphics Quality presets (`options.rs`, `PRESETS`) |
| Chat size, chat line time, max chat lines | Present | `options.rs:686-687` |
| Show HUD, hide paint/tool/brick box | Present | `options.rs` (`CHECKBOX_PREFS`), `ui/src/ui.rs` |
| Small plant errors | Present | `screens/play.rs` (`SMALL_PLANT_ERRORS`) |
| Max draw distance | Present | `options.rs` (`VISIBLE_DISTANCE_MAX`) |
| Anisotropy, trilinear, sharp filter | Present | `options.rs` (`ANISOTROPY`), `client/src/graphics.rs` |
| Precipitation | Present | `options.rs` (`PRECIPITATION`) |
| Render My Player / Render Items / Jets in first person | Missing | hidden; no reader in `crates/` |
| Sky, clouds, decals, environment maps, dynamic lights, VBO, particle falloff | Dropped | renderer-internal Torque switches |
| Audio: master, shell, sim volume | Present | `options.rs` (`VOLUMES`) |
| Play music, menu sounds, plant/move/error sounds | Present | `options.rs` (`CHECKBOX_PREFS`), `client/src/audio.rs` |
| Key remapping, Remap All, Clear All, Defaults | Present | `crates/ui/src/binds.rs:157`, `menus.rs:726` |
| Mouse sensitivity, invert mouse, keyboard turn rate | Present | `options.rs` (`SUPPORTED_CONTROLS`), `client/src/controls.rs` |
| Invert mouse in vehicles | Missing | hidden, though `client/src/app.rs:4033` reads it |
| Fast 1st/3rd switch, super-shift toggle and smart toggle | Present | `options.rs` (`CHECKBOX_PREFS`), `ui/src/ui.rs` |
| Queue brick buying, reverse brick scroll, jump/jet combo | Present | `options.rs` (`CHECKBOX_PREFS`), `ui/src/ui.rs`, `screens/selector.rs` |
| Recolour brick icons, show brick slot numbers, coloured escape menu | Present | `options.rs` (`CHECKBOX_PREFS`), `models/hud.rs` |
| Censor Chat (`$Pref::Chat::CurseFilter`) | Missing | hidden; no reader |
| Press Up to Repeat Chat (`$pref::Chat::ChatRepeat`) | Missing | hidden; no reader |
| Temp brick inside/outside uses paint colour | Missing | hidden; no reader |
| Auto Light (`$pref::Input::AutoLight`) | Missing | hidden; no reader |
| Steering auto-return, strafe steering | Missing | hidden; no reader |
| Show BL_IDs in player list | Dropped | no BL_IDs; identities are keys |
| Screenshot format | Partial | hidden; screenshots always PNG |

### Start Game (`startMissionGui`)

| Feature | Status | Evidence |
|---|---|---|
| Map list, preview and description | Present | `menus.rs:435` |
| Single player / LAN / Internet | Present | `menus.rs:298` |
| Max players, server name | Present | `menus.rs:473-538` |
| Admin and super admin passwords | Present | `menus.rs:535-536` |
| Join password | Lane | first impressions #14; hidden in `ac00991` |
| Add-Ons | Present | `menus.rs:713` |
| Advanced Config (`serverConfigGui`) | Missing | `authored_buttons` test |
| Music Files (`MusicFilesGui`) | Missing | `authored_buttons` test |
| Launch Game | Present | `menus.rs:473` |

### Join (`JoinServerGui`, `ManualJoin`, `JoinServerPassGui`)

| Feature | Status | Evidence |
|---|---|---|
| LAN query | Present | `menus.rs:745` |
| Connect by IP | Present | `menus.rs:759` |
| Favourites (in place of the internet list) | Present | `menus.rs:749-757` |
| Column sorting (8 headers) | Missing | `authored_buttons` test |
| Filters button | Missing | `authored_buttons` test; filters served the master list |
| Internet server list | Dropped | direct IP only (`docs/STATUS.md`) |
| Password prompt on join | Lane | first impressions #14 |
| Connecting and loading screens | Present | `menus.rs:405-430`; guest preview: night QA finding |

### Server settings (`serverConfigGui`, v20 `$Pref::Server::*`)

The screen exists as Admin > Host Options (`crates/ui/src/screens/admin.rs:122`),
but the host rejects every change (`crates/sim/src/session/admin.rs:601`), so
the client hides it (`crates/client/src/admin_ui.rs:361`). No setting below
is read anywhere outside `crates/admin` and the UI.

| Setting | Status | Evidence |
|---|---|---|
| Port | Present | set at host start |
| Brick limit | Lane | first impressions #14 |
| Max bricks per second | Missing | no reader |
| Max chat length | Missing | no reader; chat is not capped |
| Random brick colour | Missing | no reader |
| E-Tard (chat) filter | Missing | no reader |
| Falling damage (outside mini-games) | Missing | only mini-game rules read it (`minigames/src/policy.rs`) |
| Public domain timeout | Missing | no reader |
| Physics / player vehicle limits | Missing | no reader |
| Per-player and LAN quotas | Missing | no reader |
| Too-far distance | Missing | no reader |

### Administration

| Feature | Status | Evidence |
|---|---|---|
| Admin login, auto admin passwords | Present | `screens/admin.rs:117-126` |
| Player list: kick, ban | Present | `sim/src/session/admin.rs` |
| Unban list | Present | `screens/admin.rs`, `unBanGui` |
| Brick manager (clear a player's bricks) | Present | `screens/admin.rs`, `BrickManGui` |
| Change map | Present | `screens/admin.rs:370` |
| Admin wand, F7/F8 orb | Present | `client/src/app.rs:5008`; `3c8193b` |
| `/fetch`, `/find`, `/warp`, `/timescale` | Present | `client/src/admin_ui.rs:188-197` |
| `/realbrickcount`, `/cancelallevents`, `/clearbots`, vehicle resets | Present | `admin_ui.rs:198-202` |
| `/spy` and `/ret` | Missing | not in `admin_ui.rs:187`; goes to the Add-On router and fails |
| `/magicwand` | Partial | `/wand` works; the `/magicwand` spelling does not |
| `/getid`, `/gettransform`, `/getpz`, `/colortest`, `/tripout`, `/dfg` | Dropped | Torque debugging aids |

### Players, chat and console

| Feature | Status | Evidence |
|---|---|---|
| Say and team chat, chat HUD, page up/down | Present | `crates/ui/src/ui.rs`, remap scan |
| Talking indicator | Present | `menus.rs:677-683` |
| Emotes (`/sit`, `/love`, `/hate`, `/alarm`, `/confusion`, `/bsd`, `/hug`, `/wtf`, `/zombie`) | Present | `client/src/app.rs:5190-5196` |
| `/suicide`, `/light`, `/wand` | Present | `app.rs:5186-5189` |
| `/brickcount` | Missing | not handled |
| `/clearinventory` | Missing | not handled |
| Player list (F2), trust invite/demote, ignore | Present | `crates/ui/src/screens/players.rs:227-265` |
| Console (`~`) | Present | `crates/ui/src/screens/console.rs` |
| Center print, bottom print | Present | event outputs; `docs/audits/pong-events.md` |
| Player names over heads (F5) | Present | `crates/ui/src/ui.rs:812` |
| Net graph (Ctrl+N) | Lane | net-graph thread |

### Mini-games

| Feature | Status | Evidence |
|---|---|---|
| Create, join, leave, invite, remove, reset, end | Present | `crates/ui/src/screens/minigames.rs:227-269` |
| All 21 rule fields | Present | `crates/minigames/src/model.rs`; every field but `lives` has a reader in `sim`/`minigames` |
| Mini-game favourites (10 slots, Set Favs) | Missing | `minigames.rs` has no handler; `list_inert_buttons` |
| Scoreboard / score in player list | Present | `screens/players.rs` |

### Building

| Feature | Status | Evidence |
|---|---|---|
| Brick selector, cart, tabs, favourites | Present | `crates/ui/src/screens/selector.rs:339-348` |
| Ghost brick, shift, super shift, rotate, plant, undo, cancel | Present | remap list, all in `ui.rs` |
| Build macro record/playback | Present | `client/src/app.rs:4970-4993` |
| Paint can, colour set | Present | `sim/src/session/spray.rs` |
| FX cans (7 colour FX, 2 shape FX) | Present | `sim/src/session/tools.rs:223` |
| Print gun and print selector | Present | `screens/selector.rs` |
| Hammer, wrench | Present | `sim/src/session/tools.rs` |
| Plant error icons and sounds | Present | `screens/play.rs`, `client/src/audio.rs` |
| Warning before a bad plant | Lane | first impressions #8 |
| Brick damage from weapons | Lane | brick-damage thread |

### Wrench and events

| Feature | Status | Evidence |
|---|---|---|
| Wrench: name, light, emitter and direction, item, position, direction, respawn, raycasting, collision, rendering | Present | `crates/ui/src/screens/wrench.rs:61-74` |
| Sound wrench, vehicle spawn wrench (respawn, recolour) | Present | `wrench.rs:98-100` |
| Events dialog: add, edit, clear, send | Present | `wrench.rs:655-661` |
| All 16 input and 65 output events | Present | every name has a reader in `crates/events` or `crates/sim`; `docs/audits/pong-events.md` |
| Delayed projectile outputs | Partial | `docs/KNOWN-ISSUES.md` |

### Save and load

| Feature | Status | Evidence |
|---|---|---|
| Save with description and ownership | Present | `crates/ui/src/screens/saveload.rs:335-358` |
| Load, per map, with ownership | Present | `saveload.rs:378` |
| Sort by name or date | Present | `saveload.rs:458` |
| Fast load | Dropped | hidden, `saveload.rs:80`; no Torque ghosting |
| Colour-set mismatch prompt (`LoadBricksColorGui`) | Missing | no screen |
| Remote save warning (`saveBricksWarningGui`) | Partial | guests cannot save a host's world; no v20 warning text |

### Player appearance (`AvatarGui`, `ColorSetGui`)

| Feature | Status | Evidence |
|---|---|---|
| All parts, faces, decals, packs, hats, accents | Present | `crates/ui/src/screens/avatar.rs:664-709` |
| Part colours, colour picker | Present | `avatar.rs:694`, `avatar.rs:932-968` |
| Ten favourites, Set Favs | Present | `avatar.rs:709-721` |
| Name | Lane | first impressions #15 |

### Keys

All 81 entries of v20's remap list have a native command
(`crates/ui/src/ui.rs`, the command match from line 665), and every stock `moveMap` bind is a default
(`crates/ui/src/binds.rs`).

| Feature | Status | Evidence |
|---|---|---|
| 81 remappable actions | Present | remap scan, `ui.rs` |
| Alt+Enter fullscreen, `~` console | Present | `crates/ui/src/ui.rs:775` |
| F1 context help | Missing | ignored, `crates/ui/src/ui.rs:859` |
| F9 debug render modes | Dropped | Torque debugging |

### Gameplay feel

Sounds, particles, damage and feel of weapons, vehicles, items and the player
are the v20-fidelity thread's lane and are not rated here.

---

# Part 2: beyond v20

| Area | Rating | Evidence and what is rough |
|---|---|---|
| Hosting by direct IP, UPnP/NAT-PMP, invites, firewall rule | Solid | `docs/architecture/hosting.md`; `/invite` in `client/src/app.rs:5150` |
| Graphics presets, frame cap, FOV, MSAA, brick shadows | Solid | `options.rs` (`PRESETS`, `MAX_FPS`, `DEFAULT_FOV`, `ANTI_ALIASING`, `BRICK_SHADOWS`) |
| Music volume, mute in background | Solid | `options.rs` (`MUSIC_VOLUME`, `MUTE_IN_BACKGROUND`) |
| Autosave and unsaved-changes prompt | Solid | first impressions #3 |
| Crash reporting, version line, update check | Solid | STATUS.md, PR #15 |
| Rejoin keeps your bricks | Solid | first impressions #6 |
| Toggle crouch, side mouse buttons, draw distance | Solid | `bb41fb3`, `1932015` |
| Game modes in Start Game | Solid | `crates/ui/src/screens/modes.rs` |
| Name prompt, text size, colour-blind, subtitles, brick search, gamepad, duplicator | Lane | first-impressions thread |
| Add-On packaging, ids, `package.json`, dependencies | Solid | `docs/architecture/packages.md` |
| Add-On download on join, mismatch screen | Lane | night QA, now landing |
| Server rules in Rhai, HUD panels, weapons as data | Solid | `packages/samples/*`, `crates/package-runtime` |
| Chat commands from Add-Ons, with arguments | Solid | `9d501b7`, `crates/sim/src/session/packages.rs:1626-1680` |
| Sandboxed client code (wasm, WGSL) behind trust tiers | Solid in code, rough for modders | built in PR #11 (`319ffbe`), red-teamed (`d662c73`); the guide still lists it as "still being built" and has no walkthrough |
| v20 Add-On import (bricks, weapons, vehicles) | Solid | guide section 7; night QA |
| New bricks without v20 files | Missing | guide section 8: no native brick format |
| Block faces for `block` content | Missing | guide section 8: loads, not drawn |
| Modder tools: `bri-addon-check`, `bri-addon-run` | Solid | guide section 2 |
| Modder guide | Rough | `docs/modding/README.md:272-286` lists chat commands and client code as unbuilt; both shipped |
| Player docs: PLAYTEST.md, KNOWN-ISSUES.md | Rough | `docs/KNOWN-ISSUES.md` predates combat and vehicle work; no in-game help (Part 1, F1) |
| README | Rough | says "Windows, macOS or Linux" while STATUS.md says Windows only; says combat, vehicles and mini-games are unfinished |
| Setup from source (`tools/bootstrap.py`) | Solid | one command, reruns incrementally (README) |
| Setup for players (zip, `Launch.cmd`, SmartScreen) | Rough | no installer; SmartScreen step documented in PLAYTEST.md |

---

# Fix log

This branch fixes the most noticeable gaps first, across both parts. Each
entry names its commit. Anything too large to finish well says exactly what
is left.

(Filled in as fixes land.)
