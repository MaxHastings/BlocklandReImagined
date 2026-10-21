> **Historical note. Written 2026-09-27 (the file's date in the cloud project folder, `audits/reported-bugs-2026-09-27.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Reported bugs vs main (checked 2026-09-27 ~23:30Z)

Main checked at `5238e67`. Sources: every message Max posted in the project chat
(11:15Z to 23:05Z), the Hammer, Ramp and Tools-while-seated threads, git history
on main (175 commits), and the unmerged branches on GitHub. Code-only check: the
cloud has no v20 assets, so nothing was played.

## Was anything lost?

- **Stale-tree reverts on main:** only one. `a83d8d6` (wheel notch) undid
  `3ba01e6`, `3963e6f` and `ec9ebbc`; `bf72a36` restored them. I checked every
  line those three added: all present on main today (the only gaps are
  `regenerate_content.py` lines later reworked on purpose by `bootstrap.py`).
  The wheel-notch fix itself also survived. A whole-history scan (every commit
  whose diff resets a file to an older version, and every merge dropping a
  side) found nothing else.
- **Finished fixes that never reached main (stranded on branches):**
  - `tool-rules` (`8f0706d`, `a4c775e`): hammer won't break bricks that hold
    others up, Destructo Wand fixed. Tested on the PC, then the session dropped
    at 22:41Z before pushing. **Restored in PR #2 (https://github.com/MaxHastings/BlocklandReImagined/pull/2), left open until the Windows gate check is on main.**
  - `slides` (`14a49be`): v20 player collision so ramp slides work. Pushed to a
    branch; the full test run was never finished.
  - `breakables` (`ab6a16a`): glass map shapes smash on fast impact.

## Bug table

Status: FIXED = fix on main and still in the code. STRANDED = a fix exists on a
branch but was never merged. NOT A BUG = checked and matches v20.

| # | Bug Max reported | Status | Evidence |
|---|---|---|---|
| 1 | Duplicate shadow on Bedroom floor through a brick building | FIXED | `7c622df` occluder depth layer, all 288 lines present |
| 2 | Weird artifacts on the dark Bedroom floor | FIXED | `39a7404` stops lightmap bleeding across surface edges |
| 3 | Jets: falling pose, can't steer while jetting | FIXED | `1ed1a2b` v20 jets and air control |
| 4 | Paint can not in hand / not working | FIXED | `20ba445` tool images, `3d45293`, `42cf082` |
| 5 | Super Shift message missing | FIXED | `fff8178` |
| 6 | Hammer: no swing/particles, "out of reach", can't push vehicles | FIXED | `20ba445`; "out of reach" text is gone; `hammer_vehicle` in `tools.rs` |
| 7 | Wrench/printer "out of reach", no sounds, not usable in mid air | FIXED | `20ba445`; wrench hit/miss sounds in `tools.rs` |
| 8 | Basketball/dodgeball can't be held or thrown | FIXED | `ffb6031`, `1308da1` |
| 9 | Vehicle wheels and physics upside down | FIXED | `e0ade8d` |
| 10 | Can't scroll in menus | FIXED | `60f4f2d`, `06f9755` (you confirmed in a2) |
| 11 | Rockets don't break bricks; hammered bricks just vanish | FIXED | `ab3644c` Rapier debris, `70475a9` splash |
| 12 | No message when ending a minigame | FIXED | `79ebb04` |
| 13 | Minigame buttons: Update/Reset don't close, End before Create | FIXED | `cb18468` |
| 14 | Admin Destructo Wand doesn't work | STRANDED → PR #2 | fix in `8f0706d` on `tool-rules` (500-unit reach was rejected) |
| 15 | Hammer should refuse bricks holding others up | STRANDED → PR #2 | `8f0706d` (v20 willCauseChainKill) |
| 16 | Spy button; admin camera still turns your body | FIXED | `bc60ada` one control object per connection |
| 17 | Z free look: head doesn't turn | FIXED | `e1a89cb` |
| 18 | Dead body spins with the camera | FIXED | `bc60ada` (corpse control object) |
| 19 | Respawn message on the left, not centered | FIXED | `f6db4de` authored center/bottom print dialogs |
| 20 | Player light in the head, not the left hand | FIXED | `1308da1`, `7c02273` |
| 21 | Can't change map from the admin menu | FIXED | `1e96a3a`, `9974f44` |
| 22 | Change Map menu stays open after confirming | FIXED | `ec9ebbc`, lost in `a83d8d6`, restored `bf72a36`; all 79 lines present |
| 23 | Slopes and Slate Storm missing from Start Game | FIXED | both in `LOADABLE_MAPS` (`crates/client/src/content.rs:33,43`) |
| 24 | Can't walk up ramps or brick stairs | FIXED | `1ed1a2b` steps/slopes, `4fc2310` |
| 25 | Ramp slides (Mr.Block's Slides) don't slide | NEEDS WORK | `14a49be` on `slides` breaks brick stair climbing (`brick_staircases_and_ledges_are_climbed_without_jumping` fails every run); not mergeable as-is |
| 26 | Rendering-off bricks show translucent, should hide unless a tool is out | FIXED | non-rendering bricks drawn only with a building tool out (`app.rs:235`) |
| 27 | Tutorial rules missing | FIXED | `b23cd74`, `7c0527e` (tutorial hammer) |
| 28 | No water splash; can't swim down | FIXED | `77f14bb`, `1ed1a2b`, `e6fc3f8` |
| 29 | Emotes (/alarm, /love, /hate) show nothing | FIXED | `77f14bb` |
| 30 | No pain effect when hit | FIXED | `77f14bb` |
| 31 | No shadows on players | FIXED | `9ff91be`, `514e896` |
| 32 | Shadows on players/vehicles/tools, bricks off for now | FIXED | `514e896`, `1154a79` checkbox (default off) |
| 33 | Dropped hammer sinks into the floor; no drop sound | FIXED | `6244127`, `1f41b84` |
| 34 | Missing player types in minigames | FIXED | `da031c5` |
| 35 | Akimbo second gun points down | FIXED | `1308da1` |
| 36 | Push broom sound loops forever | FIXED | `20ba445` (image loops end with their state, `audio.rs:23`) |
| 37 | No pickup sound | FIXED | `1f41b84` |
| 38 | F zoom snaps instead of easing | FIXED | `e1a89cb` |
| 39 | Server list text cut in half | FIXED | `faea17e` |
| 40 | No name tags; Join Mini-Game list empty | FIXED | `b0e0c5b`, `d68f761` |
| 41 | Save/load: no sounds, bricks don't stream in | FIXED | `578ef69` |
| 42 | No chat talk animation | FIXED | `7f03216` |
| 43 | Can't spray paint players | FIXED | `a4f8387` |
| 44 | Glitchy walk at 45 degrees | FIXED | `1b0617b` |
| 45 | No building animations | FIXED | `0a3bd7d` |
| 46 | Grey 2x2 brick missing from hand | FIXED | `eeebfbe`, `9b872aa` |
| 47 | Tools/weapons don't work in vehicles; yaw should be fixed | FIXED | `31d944a` |
| 48 | Rider floats upright on slopes | FIXED | `f8f1e5f` |
| 49 | Dizzy camera when turning | FIXED | `879b923` (view was ~121° wide, now v20's 90°) |
| 50 | Silent Jeep dismount | NOT A BUG | v20 has no dismount sound (checked in the exe by the seats thread) |
| 51 | Hips float above the shoes | NOT A BUG | you confirmed it's authentic v20 |
| 52 | Fullscreen / resolution glitches | FIXED | `1eb0bdd` borderless fullscreen, `7f677d4` mouse recapture |
| 53 | Main menu text stretches with the window | FIXED | `f10c4a5` |
| 54 | Horse camera and rider animations wrong | FIXED | `73cd371`, `d27a9b5` |
| 55 | Akimbo guns fire too fast | NEEDS PC CHECK | suspect: the left gun's trigger is set on `onFireAkimbo` (`crates/weapons/src/runtime.rs:1095`) and only cleared on the next press (`runtime.rs:672-679`), so the left gun can keep auto-firing; the pack-backed test `gun_is_semi_auto_and_akimbo_fires_on_release` (`crates/weapons/tests/runtime.rs:268`) only watches 30 ticks after release |
| 57 | "Dynamic scene topology changed" crash | FIX INCOMING | avatar mesh (`crates/client/src/avatar.rs:844`) calls `update_vertices` with a changed layout; an outside PR re-uploads it. The only other caller, world items (`world_items.rs:706`), already checks topology first |
| 56 | Jeep turning dizziness on foot too | FIXED | same FOV fix, `879b923` |

Asks from threads that were later deleted (only the agents' acks survive), all
with a commit on main: undo (`d8b8bb4`), FOV option (`43c6273`), tank turret
(`3963e6f`, restored `bf72a36`), printer (`988b43a`), mouse capture (`7f677d4`),
~ console (`a92a3fe`), transparent spray can (`42cf082`), scroll wheel notch
(`a83d8d6`), tutorial hammer (`7c0527e`), flying jeep (`e9e85da`), loading
progress (`c092117`), player light flare (`7c02273`), river water bricks
(`060e3f6`). Breakable map objects is STRANDED on `breakables` (`ab6a16a`). It conflicts with main's streamed world transfer (`protocol.rs`, `server.rs`) and the motor, so it needs a re-port, not a cherry-pick.
