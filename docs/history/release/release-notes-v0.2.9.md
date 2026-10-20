> **Historical note. Written 2026-10-11 (the file's date in the cloud project folder, `release/release-notes-v0.2.9.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

Blockland ReImagined v0.2.9 Alpha.

## New
- **Environment favourites.** The Environment window (Admin > Environment) has ten Favorites slots. Store a look, Load it later, on any server or map. Changes now reach the server a moment after you stop moving a slider, so everyone sees them without pressing Apply (Apply stays as send-now).
- **Add-On downloads show on the loading screen.** Joining a server with Add-Ons you don't have no longer sits behind a frozen "Connecting" dialog. You see `DOWNLOADING ADD-ONS`, `VERIFYING ADD-ONS` and `LOADING ADD-ONS` with sizes, then the usual world stages.
- **Better shadows at Medium lighting.** Sun shadows now reach the far side of big maps (the far wall of the Bedroom is no longer flat bright), and Ambient Occlusion costs about half as much.
- **Item pickups can be seen by events and Add-Ons.** Spawn bricks fire a new `onItemPickup` event after an item is really taken, and the wrench has a "Respawn only on mini-game reset" box. Add-On commands no longer charge a player for something they were never given.

## Fixes
- Fixed the soccer crash: a push, hit or hold on a ragdoll limb removed before the next physics step could take the hosting player's game down. An Add-On rule that removes its own item spawner on pickup could also crash the host.
- Bots: far cheaper host tick (at 32 bots, ticks over budget drop from 30% to 12% and the worst tick from 86 ms to 37 ms, which is what made bots appear to teleport). The "Empty slot" bot errors are gone, and an Add-On hook can no longer crash the host by removing a bot mid-turn.
- Changing Shadow Quality, anti-aliasing or colour vision no longer freezes the game for most of a second on a big build (764-962 ms down to 87-91 ms on a 187,762-brick save).
- Add-On compatibility: script math accepts whole numbers and the usual function names (Rallypack's far-shot `ceil` error), calls to missing functions are reported when the Add-On loads, old saves' NOPRINT bricks print again, goals can push objects when their builder is away, and the P90 and other Add-On shapes no longer draw opaque parts as see-through. Rallypack and P90 port recipes are now in the repository.
- Soft Shading no longer hitches while walking or building: the skylight is rebuilt over several frames.

## Under the hood
- Session logs now explain frame spikes: long frames are named with the work behind them, and frames spent alt-tabbed are counted separately instead of reading like a slow GPU.

## Deferred
- The Enhanced Sky setting stays shelved and is not in this release.

## Known issues
- See [KNOWN-ISSUES.md](https://github.com/MaxHastings/BlocklandReImagined/blob/v0.2.9/docs/KNOWN-ISSUES.md).

Everything else is as in [v0.2.8](https://github.com/MaxHastings/BlocklandReImagined/releases/tag/v0.2.8) (its known issues included).

## Download
| Platform | File | Launch |
|---|---|---|
| Windows x86-64 | `BlocklandReImagined-windows.zip` | `Launch.cmd` |
| macOS Apple silicon | `BlocklandReImagined-macos.zip` | `BlocklandReImagined.app` (ad-hoc signed) |
| Linux x86-64 | `BlocklandReImagined-linux.zip` | `./launch.sh` (glibc 2.35 or newer) |

Extract into a fresh folder. Settings and saves stay in your normal user profile. Host multiplayer by IP; internet hosting needs UDP port 28000 reachable. `SHA256SUMS` has the archive hashes.
