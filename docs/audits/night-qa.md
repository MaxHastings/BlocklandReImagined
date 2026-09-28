# Night QA of alpha a13

Overnight headless QA of `dist/BlocklandReImagined-alpha-2026-09-28-a13`
(main `0d330c0`, protocol 35), 2026-09-28. Everything ran against a scratch
copy of the build's `content/` with the harness in
`crates/client/tests/night_qa.rs`: real `App`s (host and guests) over
loopback, synthetic UI actions, offscreen GPU captures, no window and no OS
input. The shipped folder was never touched. Screenshots and per-screen text
dumps are under `artifacts/night-qa/` in the night-qa worktree (not
committed).

Run the harness (each test is `#[ignore]`):

```sh
BRI_CONTENT_ROOT=<build>/content BRI_QA_OUT=<dir> \
  cargo test -p bri-client --release --test night_qa -- --ignored --nocapture <test>
```

| Test | What it does |
|---|---|
| `every_map_every_mode_two_players_build_save_reload_change_map` | LAN host plus a guest on every stock map under every game mode: plant, save, hammer, reload, hammer and replant, Change Map. `BRI_QA_MAPS=slate,bedroom` narrows it. |
| `new_player_screens` | First run to disconnect: every menu and dialog captured with its visible text. |
| `imported_v20_add_ons_play` | Import button worker plus `bri-import-addon.exe`, turn both on, plant the imported brick, fire the imported weapon. Needs `BRI_IMPORT_ROOT` and `BRI_IMPORTER`. |
| `soak_four_players_build_drive_fire_chat_and_save` | Host and three guests for `BRI_SOAK_SECONDS` (default 3600) with a save, clear and reload every `BRI_SOAK_SAVE_SECONDS` (900). |
| `click_places_the_ghost_after_the_brick_is_in_hand` | Regression test for fix 2. |
| `plant_probe_single_player` | Plants in four directions and prints what the player is told. `BRI_QA_PROBE_MAP`, `BRI_QA_PROBE_MODE=1`. |
| `stress_lab_single_player_build_save_reload` | Stress Lab Strata plant, save and reload in single player. |

Creating `<dir>/PAUSE` makes the matrix release UDP 28000 and wait, so a
gate run can use the ports.

## Fixed on `claude/night-qa`

Fix commits (each with a test) versus harness and doc commits:

1. **Options stored v20's 800x600 and VSync off** (`79eeabb`, on main).
   A first Options visit showed 800x600 and "Disable Vsync" ticked (the v20
   UI pack's stock prefs), and Done saved them, so the next launch opened an
   800x600 window with VSync off and no frame cap. Test
   `stock_v20_display_defaults_are_not_shown_or_saved`.
2. **Release blocker: clicking never placed a ghost once the brick was in
   hand** (`a0069ca`, on main). The server mounts the grey brick image one
   round trip after a brick is chosen; the client took it for a held item
   and sent every click to its trigger. Picking a brick, then clicking, did
   nothing. Existing tests clicked in the same frame as choosing the brick,
   so they never saw it. `click_places_the_ghost_after_the_brick_is_in_hand`
   fails on a13 code and passes with the fix.
3. **The host's firewall question was cut off** (`e0ee245`, on main). The
   Yes/No box is authored one line tall; it read only "Windows Firewall would
   stop friends from". Message boxes now grow to their text like v20's
   `MBSetText`. Test `crates/ui/tests/message_box.rs`.
4. **An Add-On's bricks could not be chosen** (`ec6072a`). An imported brick
   pack showed in the brick selector, but the building tools only accepted
   the base catalog, so choosing one did nothing. Proven by
   `imported_v20_add_ons_play` (fence planted, shotgun fired).
5. **"No weapon image equipped" printed on screen** (`8b62e2a`). A click
   that reached the server just after Change Map or a respawn cleared the
   hands came back as that developer message in the bottom print (seen after
   every Change Map in the matrix). A click with nothing in hand is now a
   silent no-op, as a release already was; the session test
   `release_after_core_switch_is_idempotent_but_cannot_start_a_weapon` now
   asserts that.

## Map × mode matrix (definition of done, item 1)

LAN host plus one guest; on each map the guest plants a 2x4, the host saves,
the guest hammers it, the host reloads, the guest hammers and replants; then
the host uses Change Map (Host where the previous visit failed). Stock
Add-Ons enabled as shipped. Game modes offered: Custom and Stress Lab.

| Mode | Map | Reached by | Result | Console warnings |
|---|---|---|---|---|
| Custom | Bedroom | Host | pass | 0 |
| Custom | Kitchen | Change Map | pass | 0 |
| Custom | The Slopes | Change Map | first plant near spawn refused as Buried (finding A) | 0 |
| Custom | Slate | Host | pass | 0 |
| Custom | Bedroom - Dark | Change Map | pass | 0 |
| Custom | Construct | Change Map | pass | 0 |
| Custom | Destruct | Change Map | pass | 0 |
| Custom | Halloween Slate | Change Map | pass | 0 |
| Custom | Kitchen - Dark | Change Map | pass | 0 |
| Custom | Skylands | Change Map | pass | 0 |
| Custom | Slate Desert | Change Map | pass | 0 |
| Custom | Slate Sea Revised | Change Map | plant, save and reload pass; the hammer cannot reach (harness: the brick lands on the seabed 7 units below the player) | 0 |
| Custom | Slate Storm Revised | Host | as Slate Sea | 0 |
| Custom | Tutorial | Host | the guest is refused (finding B) | 1 |
| Stress Lab | Strata | Host | the guest cannot plant on the generated ground (finding C) | 0 |

No visit spammed the console: no line repeated five or more times on any
map. The matrix ran the a13 content with fixes 1 to 4 in the code; without
fix 2 a guest cannot plant after the first round trip, so the a13 code fails
every visit.

## Soak: one hour, four players (definition of done, item 4)

Host plus a builder (a brick every 2 s, hammering and walking), a gunner in a
mini-game firing the gun about once a second, a jeep driver weaving circles,
and chat from everyone every 20 s, on Slate over LAN. Every 15 minutes the
host saved, cleared all bricks and reloaded, checking every player's count.

| Measure | Result |
|---|---|
| Duration | 3608 s (59 full minute samples) |
| Disconnects | none; all four players in game every minute |
| Save, clear, reload | 4 of 4 exact (92, 178, 216 and 247 bricks at the end); the reload reached every player in 0.4 to 1.1 s |
| Server ticks per minute | 7086 to 7314 (120 Hz is 7200; the spread is sampling jitter around the save pauses) |
| Client step time (per app, wall) | 0.40 to 0.64 ms average; worst single step 13 ms (host) |
| Process memory (four apps and the host server) | 694 MB working set at the start, 726 MB at the end; private 682 to 709 MB |
| Console warnings / errors | 0 / 0 |
| Activity | 1449 plant attempts (247 bricks standing at the end), 2898 trigger presses, 576 chat lines, 2581 s in the jeep |

All four apps ran in one process with no window, so the step times are the
simulation and presentation cost without drawing; frame time on a GPU is not
covered. `artifacts/night-qa/soak/soak.json` has the per-minute rows and
`memory.csv` the per-minute memory. The soak ran with fixes 1 to 4.

## Findings not fixed

A. **The Slopes: the first brick near spawn is refused as Buried.** At the
   spawn the ground slopes; a 2x4 aimed at the ground in any of four
   directions snaps partly into the terrain and the server answers Buried
   (the plant-error icon shows). A new player who starts on The Slopes cannot
   place a first brick near spawn without finding flatter ground. Whether v20
   allowed it here was not checked. Repro: `plant_probe_single_player` with
   `BRI_QA_PROBE_MAP=v20/add-ons/map_slopes/slopes.mis`.
B. **Tutorial hosted as LAN refuses every joiner** with "The server refused
   the join: Player spawn is obstructed". The Tutorial is in the Start Game
   map list with the LAN and Internet options, but it has one spawn. Either
   keep it single player in Start Game, or word the refusal for players.
C. **Stress Lab Strata: a joining player cannot build on the generated
   ground.** The host plants fine in single player (`BRI_QA_PROBE_MODE=1`); a
   guest's plant never appears and shows no plant-error icon. The generated
   blocks belong to the host, so this is likely the trust rule; the missing
   icon makes it look broken. Needs a decision: world blocks public, or an
   icon and message.
D. **The Stress Lab Miner HUD shows on every map.** With the shipped
   Add-Ons, hosting Slate (Custom) shows the Stress Lab Miner panel with its
   H, G and J keys (`19-in-game.png` in the new-player shots). Consider
   shipping the Stress Lab Add-Ons turned off, or showing the HUD only when
   its server Add-On runs.
E. **Saving in Stress Lab Strata saves the generated ground.** A save there
   held 14 897 bricks, almost all generated blocks; loading it into a fresh
   Strata world with a guest appended about 15 000 bricks on top of the
   regenerated ground (29 771 bricks afterwards).
F. **Slate's ground is not drawn in offscreen captures.** Every Slate
   variant rendered sky below the horizon while players stood on the ground
   (The Slopes' terrain draws). This may be the headless capture path only;
   worth one look in a real window before the home test.
G. **The Import button runs without a v20 reference.** An imported weapon
   that borrows stock sounds or effects (Weapon_Shotgun uses Weapon_Gun's
   shot sound, flash and bullet) loses them: its report lists 8 ambiguous
   references. The command-line importer takes `--reference`; the button has
   none to give.
H. **The guest loading screen has no map name or preview**
   (`28-guest-loading.png`: sky and "RECEIVING WORLD"); the host's loading
   screen shows both.
I. **Connect to IP has no hint** about the accepted forms (address, host
   name, port, invite); PLAYTEST.md documents them.
J. **Doc drift:** `docs/modding/README.md` section 8 still calls the Import
   Add-On button "coming soon"; it ships in a13.

## New-player path and the 20 first-impressions items (item 1)

Captured offscreen by `new_player_screens` from an empty state folder: first
run, main menu, Options (each tab), Start Game, Game Mode, Add-Ons, Join
Server, Connect to IP, Avatar, About, Console, connecting to an address
nobody answers, hosting (loading, in game, Escape, player list, brick
selector, mini-games, save, load, admin), a second player joining by address
(connect, loading, in game) and the host quitting. Double-clicking the
executable, the Windows firewall prompt itself and crash dialogs need a real
window and were not run.

| # | Item | Tonight |
|---|---|---|
| 1 | Double-click; visible startup errors | Can't check (needs a window). |
| 2 | Crashes are not silent | Can't check. |
| 3 | Unsaved work | Not exercised. |
| 4 | Damaged settings | Not exercised. |
| 5 | Joining by IP | Pass: Connect to IP joins by address; no answer reads "No server answered at 127.0.0.1:28999. Check the address, that the server is running, and that UDP port 28999 is open on the host's router and firewall." |
| 6 | Rejoin and reconnect | Not exercised (no drops in the soak). |
| 7 | One bad save | Not exercised. |
| 8 | Warning before a bad plant; undo | Fail: still no warning before planting (The Slopes shows Buried only after the server answers). Found and fixed the no-ghost blocker. Undo not exercised. |
| 9 | Readable disconnect text | Pass for the host quitting ("The host closed the server.") and no answer; the Tutorial join refusal is developer wording (finding B). |
| 10 | Loading feedback | Partial: the host's loading screen is fine; the guest sees no map name or preview (finding H). |
| 11 | Tutorial end to end | Not checked; hosting it for two players fails (finding B). |
| 12 | Frame cap, presets, render distance | Partial: Quality presets and Max FPS are there; no render distance. Fixed the 800x600 and VSync-off defaults. |
| 13 | Text size, colourblind, subtitles | Fail: none in Options. |
| 14 | Join passwords, brick limits | Open (Max's decision). |
| 15 | Name prompt; duplicate names | Fail: no prompt on first run; the guest joined as "Blockhead". |
| 16 | Dedicated server persistence | Not checked. |
| 17 | Version, updates, symbols | Fail on a13 (the main menu reads "ReImagined — development"); main has had the version corner since, not verified here. |
| 18 | Brick search, duplicator | Fail: the brick selector has no search; no duplicator. |
| 19 | Input options | Fail: no hold or toggle choice for crouch, walk or jet (only the Super Shift toggle); no gamepad. |
| 20 | Music slider, live preview | Pass: Music Volume slider with readouts. |

## v20 Add-On imports (item 2)

- **Import button:** `Weapon_Shotgun.zip` (community archive) dropped in
  `content/Add-Ons` shows as "Not Imported Yet"; the button's worker imports
  it ("Imported Weapon_Shotgun. It starts off; its report is
  addons/weapon_shotgun/IMPORT-REPORT.md."), it lists as Sawn-off Shotgun
  under Weapons & Items, and turns on.
- **`bri-import-addon.exe`:** `Brick_Fence.zip` with `--reference` to the v20
  install converts cleanly (30 ids, no gaps) and appears turned off.
- **Playing:** in a hosted game the fence brick is in the brick selector and,
  after fix 4, plants; the shotgun from a mini-game loadout is held and fires
  (`import/imported-shotgun.png`).
- Not covered: a guest joining a host with imported Add-Ons (download, then
  build with them).
