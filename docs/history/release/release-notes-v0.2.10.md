> **Historical note. Written 2026-10-12 (the file's date in the cloud project folder, `release/release-notes-v0.2.10.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

Blockland ReImagined v0.2.10 Alpha.

## New
- **Bots drive and crew vehicles.** Bots now stay in a jeep or tank and use it: drivers explore, run people over and stop before drops, allies take the offered seats and crew the tank, and a bot only gets out for a reason (wrecked, stalled, a shorter walk, off to fight and so on). Bots also call out "Get in!" when offering a seat and "Move!" when an ally is in their line of fire.
- **One-step Add-On porting.** `python tools/port_addon.py build <zip> --out <dir>` imports a Rallypack-style Add-On, keeps only the items you pick and what they need, checks it, packages it and can install it. A ported Add-On ZIP dropped in the Add-Ons folder now installs as a ported Add-On (starting off) instead of going to the classic importer. The Add-On checker also names weapon timing mistakes (zero-time cycles, idle loops, same-step callbacks) before you play.
- **Bots use script-fired guns properly.** Bots can see a scripted gun's rounds, never fire it empty, and switch to another weapon when it runs out. The Rallypack and P90 recipes declare their ammo.
- **Environment colour pickers apply live.** In Admin > Environment, moving a colour picker's sliders (Shadow Color, Sun Flare, Ambient and Direct light, Fog, Sky, Vignette) changes the world straight away, like the main sliders. Cancel or Escape puts the colour back.
- **Ctrl + mouse wheel pages through lists**, such as the Brick Selector, a page at a time.
- **Chat opens over Help, the Player List and the Esc menu.**

## Changes
- Rejoining a server you just left now downloads only the bricks that changed while you were away, instead of the whole world. If anything doesn't line up, it falls back to the full download.
- **Simpler scoring for soccer and coin games**. A new mini-game scores +1 for a kill and nothing else: Kill Self now defaults to 0, Slayer's Friendly Fire to 0 and Bonus Kills to off. Setting Kill Player to 0 makes every kill worth nothing, sprees and bot kills included. A new Slayer setting, Bots Worth Kill Player (on by default), scores bots like players; turn it off to set Kill Bot separately. Objective points (captures, flags, goals) are unchanged.
- **Wrench:** "Respawn only on mini-game reset" now sits on the Item Respawn Time row as an "On reset" box, and ticking it greys out the seconds.
- **Loading a save with missing Add-Ons** posts one short chat line and opens a message box listing what is missing, grouped by Add-On, with the IDs to install or enable.
- **Hosting behind a router that refuses UPnP** no longer says friends cannot join. It says automatic port opening failed, friends can still join if the port is forwarded by hand, and how. Logged router errors no longer contain addresses or invite keys.

## Fixes
- A vehicle wheel spinning very fast in the air could break the vehicle's physics and drop players from the server. It no longer does.
- Bots stepping round an ally no longer walk off the edge of a deck or rooftop.
- Big bot games: with 32 bots on a map full of walls, bots no longer stand at their spawns waiting for a route (stuck time in the first 30 seconds drops from 46% to 4%).
- Bots with swords no longer kill a teammate who steps into the swing during its wind-up.
- Bots jump straight into a low crawlspace one step up instead of pressing against it for a second.
- Add-On shops and other scripted rules check 13 more operations (placing bricks, dropping items, damage, heal, teleport, ammo, score, spawning and more) before anything happens, so a player is not charged for something that cannot be handed over. Vehicles, mini-game rules, bots, copying bricks and placing voxels are now checked the same way.
- A crash-recovery snapshot that was damaged on disk is now set aside when you host again, so the new game is protected by recovery instead of running without it until you pick Discard.
- Rejoining a server after a crash no longer drops a whole downloaded Add-On because one of its files was damaged; only that file is fetched again.
- Leftover temporary files from an interrupted world-recovery save are cleared when a server starts.

## Under the hood
- Servers do less repeated work each tick for weapon and ray lookups and for Add-On scripts, which matters most with many bots firing. Parked vehicles nobody is touching now sleep instead of being simulated every tick. Placing or breaking a brick now updates bot routes only around that brick, not across the whole area.
- Particle emitters out of sight now sleep, and they refill when you look back, so you see a full cloud. On a large build this cut simulated particles by up to 87% and spawn frames by about 0.8 ms. Builds with effects bricks also update faster when bricks change.
- The game no longer re-runs its collision check every frame when nobody else has moved. Single-player and quiet servers do less work per frame.
- Rendering does less repeat work: player and horse models are built only where they can be seen, sun shadow layers are reused while nothing in them moved, and record passes skip unchanged work.
- Saving a world now happens off the server's game tick, so a save on a big build no longer stalls everyone's game while it is written.
- Bots look up who is friend or foe once per tick instead of once per bot, which takes 9 to 22% off a busy bot tick.
- New world-safety, endurance and large-build stress tests: recovery snapshots survive a host being killed mid-write, sixty join/build/save/map-change cycles lose nothing, and save, load, tick and edit cost is measured up to a million bricks.
- Test tiers (`python tools/check.py quick|focused|full|soak`) for faster checks while working; the push gate is unchanged.
- Several tests no longer time out on slow machines.

## Behaviour changes
- Add-On favourites keep their numbers, except that a favourite whose Kill Bot differs from Kill Player now scores bots like players until Bots Worth Kill Player is turned off.
- Saved builds store Add-On settings only where they differ from the defaults, so a build that relied on Bonus Kills being on, Friendly Fire at -3 or Kill Self at -1 now loads with the new defaults.

## Players on older versions
- v0.2.10 uses a new network protocol, so v0.2.9 and older clients cannot join a v0.2.10 server, and a v0.2.10 client cannot join an older server. Everyone playing together needs v0.2.10.

## Deferred
- The Enhanced Sky setting stays shelved and is not in this release.

## Known issues
- Bots that drive vehicles get stuck slightly more often in our obstacle-course test (3.4% of the time, up from 2.0%, still under our 5% limit). If you see a bot stuck at the wheel, let us know.
- Heavy gunfights between 16 bots on Beta City can stutter (some host ticks take 15 to 35 ms). This was already the case in v0.2.9.
- See [KNOWN-ISSUES.md](https://github.com/MaxHastings/BlocklandReImagined/blob/v0.2.10/docs/KNOWN-ISSUES.md).

Everything else is as in [v0.2.9](https://github.com/MaxHastings/BlocklandReImagined/releases/tag/v0.2.9) (its known issues included).

## Download
| Platform | File | Launch |
|---|---|---|
| Windows x86-64 | `BlocklandReImagined-windows.zip` | `Launch.cmd` |
| macOS Apple silicon | `BlocklandReImagined-macos.zip` | `BlocklandReImagined.app` (ad-hoc signed) |
| Linux x86-64 | `BlocklandReImagined-linux.zip` | `./launch.sh` (glibc 2.35 or newer) |

Extract into a fresh folder. Settings and saves stay in your normal user profile. Host multiplayer by IP; internet hosting needs UDP port 28000 reachable. `SHA256SUMS` has the archive hashes.
