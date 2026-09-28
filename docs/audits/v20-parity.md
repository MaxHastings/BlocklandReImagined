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
| Part 1 on `1932015` (120 rows) | 74 | 5 | 27 | 7 | 7 |
| Part 1 now (120 rows) | 100 | 5 | 0 | 7 | 8 |

At the audit, the misses a player noticed first were, most noticeable first:

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

Items 1 to 4, 6 and 7, and the vehicle mouse invert in item 5, are fixed
on this branch; the rest of item 5 and item 8 are open. The fix log at the
end lists every change and what is still open.

Part 2 in one line: the modern layer is **solid** on hosting, Add-On
packaging, the sandbox and safety, and now on documentation; it is
**missing** a native brick format, drawn block faces and the join-screen
prompt for trusting client code. Details are in Part 2.

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
| Credits (`getHelp("1. Credits")`) | Present | fixed in `37a40b3`: HelpDlg with the eight v20 pages |
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
| Render My Player / Render Items / Jets in first person | Present | `14601a6b`: Show Jets in First Person in Options; Render Items honoured from its pref (v20 authored its checkbox off the pane); Render My Player dropped, as v20's datablock never draws the body in first person |
| Sky, clouds, decals, environment maps, dynamic lights, VBO, particle falloff | Dropped | renderer-internal Torque switches |
| Audio: master, shell, sim volume | Present | `options.rs` (`VOLUMES`) |
| Play music, menu sounds, plant/move/error sounds | Present | `options.rs` (`CHECKBOX_PREFS`), `client/src/audio.rs` |
| Key remapping, Remap All, Clear All, Defaults | Present | `crates/ui/src/binds.rs:157`, `menus.rs:726` |
| Mouse sensitivity, invert mouse, keyboard turn rate | Present | `options.rs` (`SUPPORTED_CONTROLS`), `client/src/controls.rs` |
| Invert mouse in vehicles | Present | fixed in `828b4d9`; its pitch sign fixed in `394a6bc` |
| Fast 1st/3rd switch, super-shift toggle and smart toggle | Present | `options.rs` (`CHECKBOX_PREFS`), `ui/src/ui.rs` |
| Queue brick buying, reverse brick scroll, jump/jet combo | Present | `options.rs` (`CHECKBOX_PREFS`), `ui/src/ui.rs`, `screens/selector.rs` |
| Recolour brick icons, show brick slot numbers, coloured escape menu | Present | `options.rs` (`CHECKBOX_PREFS`), `models/hud.rs` |
| Censor Chat (`$Pref::Chat::CurseFilter`) | Present | fixed in `7caf41e4` |
| Press Up to Repeat Chat (`$pref::Chat::ChatRepeat`) | Present | fixed in `7caf41e4` |
| Temp brick inside/outside uses paint colour | Present | fixed in `7caf41e4`, with the flash time and colours |
| Auto Light (`$pref::Input::AutoLight`) | Present | checkbox in `7caf41e4`; behaviour from final touches |
| Steering auto-return, strafe steering | Present | checkboxes in `14601a6b`; behaviour in `cdd40260` (final touches) |
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
| Advanced Config (`serverConfigGui`) | Present | fixed in `6a3e230`: saved `$Pref::Server::*`, applied when hosting |
| Music Files (`MusicFilesGui`) | Present | fixed in `49214a2`: tracks left off are not offered to music bricks; joiners' wrenches list only the offered ones since `2ce1ecd2` |
| Launch Game | Present | `menus.rs:473` |

### Join (`JoinServerGui`, `ManualJoin`, `JoinServerPassGui`)

| Feature | Status | Evidence |
|---|---|---|
| LAN query | Present | `menus.rs:745` |
| Connect by IP | Present | `menus.rs:759` |
| Favourites (in place of the internet list) | Present | `menus.rs:749-757` |
| Column sorting (8 headers) | Present | fixed in `04fe08f` |
| Filters button | Dropped | hidden in `04fe08f`; filters served the master list |
| Internet server list | Dropped | direct IP only (`docs/STATUS.md`) |
| Password prompt on join | Lane | first impressions #14 |
| Connecting and loading screens | Present | `menus.rs:405-430`; guest preview: night QA finding |

### Server settings (`serverConfigGui`, v20 `$Pref::Server::*`)

On `1932015` the host refused every change and no setting was read. Since
then the first-impressions thread applied the brick limit, plant rate,
chat length and reach (`d22e5d6`, Admin > Server Settings), and this branch
opened Start Game's Advanced Config and applied the rows marked fixed.

| Setting | Status | Evidence |
|---|---|---|
| Port | Present | set at host start |
| Brick limit | Lane | first impressions #14 |
| Max bricks per second | Present | `d22e5d6` (first-impressions thread) |
| Max chat length | Present | `d22e5d6` (first-impressions thread) |
| Random brick colour | Present | fixed in `b3409a9`; the ghost shows the next colour since `c72cc900` |
| E-Tard (chat) filter | Present | fixed in `e9fd8ed8` |
| Falling damage (outside mini-games) | Present | fixed in `74b8efa`; on by default as in v20 |
| Public domain timeout | Present | fixed in `e9fd8ed8` |
| Physics / player vehicle limits | Present | fixed in `74b8efa` |
| Per-player and LAN quotas | Present | vehicles in `e9fd8ed8`; schedules, lights and emitters, items and projectiles in `fbc263ef`. Misc counts explosions, which are instantaneous here |
| Too-far distance | Present | `d22e5d6` (first-impressions thread) |

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
| `/spy` and `/ret` | Present | fixed in `71b548f` |
| `/magicwand` | Partial | `/wand` works; the `/magicwand` spelling does not |
| `/getid`, `/gettransform`, `/getpz`, `/colortest`, `/tripout`, `/dfg` | Dropped | Torque debugging aids |

### Players, chat and console

| Feature | Status | Evidence |
|---|---|---|
| Say and team chat, chat HUD, page up/down | Present | `crates/ui/src/ui.rs`, remap scan |
| Talking indicator | Present | `menus.rs:677-683` |
| Emotes (`/sit`, `/love`, `/hate`, `/alarm`, `/confusion`, `/bsd`, `/hug`, `/wtf`, `/zombie`) | Present | `client/src/app.rs:5190-5196` |
| `/suicide`, `/light`, `/wand` | Present | `app.rs:5186-5189` |
| `/brickcount` | Present | fixed in `71b548f` |
| `/clearinventory` | Present | fixed in `c72cc900`: empties the brick cart |
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
| Mini-game favourites (10 slots, Set Favs) | Present | fixed in `a7e161f` |
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
| Colour-set mismatch prompt (`LoadBricksColorGui`) | Present | fixed in `c3ad84a0`: Nearest Match, Add More Colors, Cancel. Replace is hidden: v20 needed it for a 64-colour set, ours holds 256, and the replicated palette only grows |
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
| F1 context help | Present | fixed in `37a40b3` |
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
| Sandboxed client code (wasm, WGSL) behind trust tiers | Solid | built in PR #11 (`319ffbe`), red-teamed (`d662c73`); joining asks "Trust and join" since `8b009925`. Elevated code is not offered to joiners yet (guide section 8) |
| v20 Add-On import (bricks, weapons, vehicles) | Solid | guide section 7; night QA |
| New bricks without v20 files | Missing | guide section 8: no native brick format |
| Block faces for `block` content | Missing | guide section 8: loads, not drawn |
| Modder tools: `bri-addon-check`, `bri-addon-run` | Solid | guide section 2 |
| Modder guide | Solid | updated in `b639104`: chat commands and client code are no longer listed as unbuilt; client code has a section |
| Player docs: PLAYTEST.md, KNOWN-ISSUES.md, in-game help | Solid | KNOWN-ISSUES updated in `b639104`; F1 help in `37a40b3` |
| README | Solid | updated in `b639104`: Windows only; the playtest paragraph describes the current build |
| Setup from source (`tools/bootstrap.py`) | Solid | one command, reruns incrementally (README) |
| Setup for players (zip, `Launch.cmd`, SmartScreen) | Rough | no installer; SmartScreen step documented in PLAYTEST.md |

---

# Fix log

Hashes are this branch's; Gate lands each under a new hash on main.

| Commit | What changed |
|---|---|
| `6a3e230` | Start Game's Advanced Config opens v20's serverConfigGui over the saved `$Pref::Server::*`; a hosted game starts with them and keeps them across Change Map. |
| `04fe08f` | Join Server's column headers sort like `JS_sortList`; Filters and the query's Cancel are hidden. |
| `828b4d9` | Options shows Controls' Invert Mouse In Vehicles (on by default). |
| `37a40b3` | The Credits button and F1 open HelpDlg with v20's eight help pages (the UI importer carries `base/help/*.hfl`). |
| `403b9eb` | Max's report: server list rows drew ServerListProfile's black outline around black text; list rows now draw plain. |
| `394a6bc` | Max's report: mouse-steered vehicles pitched the wrong way under the default Vehicle Mouse Invert. |
| `7f05b74`, `361b2f0` | Max's report: every Options tab fits inside the dialog at any window shape; a window under 640x480 scales the UI down. |
| `71b548f` | `/brickcount` for everyone, `/spy <name>` and `/ret` for admins. |
| `320d6a0` | Max's report: windows drag by their title bar (`canMove`) and stay on screen. |
| `b639104` | README, Known Issues and the modding guide match what shipped. |
| `74b8efa` | Falling Damage outside mini-games follows the host's setting (on, as in v20); the physics and player vehicle totals hold back spawns. |
| `b3409a9` | Random Brick Color colours planted bricks from v20's six. |
| `a7e161f` | Create Mini-Game's ten favourite slots and Set Favs. |
| `49214a2` | Start Game's Music Files; a hosted game offers only the tracks left on. |
| `7caf41e4` | Options shows Censor Chat, Press Up to Repeat Chat, the temp brick paint colours and flash, and Auto Light, each honoured in game. |
| `14601a6b` | Show Jets in First Person and Render Items are honoured; Options shows Strafe Steering and Steering Auto-Return. |
| `9ea1b6f6` | Help pages draw their fonts, colours and margins (the UI's ML text). |
| `2497a46d` | Windows v20 authored resizable, maximizable or minimizable (Join Server and others) resize from their edges and use their title bar boxes. Options stays fixed, as its tabs are laid out for one size. |
| `e9fd8ed8` | The E-Tard filter, the public domain timeout and the per-builder vehicle quotas apply. |
| `fbc263ef` | Schedule, light and emitter, item and projectile quotas apply per builder, LAN or Internet, clamped as v20's `verifyQuotaNumber`. |
| `c3ad84a0` | Load Bricks asks how to load a save's differing colours (`LoadBricksColorGui`). |
| `c72cc900` | Random Brick Color shows the next colour on the ghost; `/clearinventory` empties the brick cart. |
| `2ce1ecd2` | A joiner's wrench lists only the music the host's Music Files offer. |

`crates/ui/tests/authored_buttons.rs` now asserts that no visible button on
the 19 menu screens answers "Interface under construction".

## Still open

Every Part 1 row that was missing is built. What remains is partial or
dropped, each for the reason in its row:

- **Partial**: the lighting/particle/texture quality radios (replaced by
  Graphics Quality presets), the screenshot format (always PNG),
  `/magicwand` (v20's admin wand, which destroys anyone's bricks, is not
  built; `/wand` works), delayed projectile outputs
  (`docs/KNOWN-ISSUES.md`) and the remote save warning text.
- **Dropped this round**: Load Bricks' Replace Current Color Set (hidden;
  see its row), Render My Player (see its row) and the Misc quota (it
  counts explosions, which are instantaneous here).

Other lanes' items (join password, name prompt, net graph, weapon feel,
brick damage, Add-On downloads) stay with those threads.
