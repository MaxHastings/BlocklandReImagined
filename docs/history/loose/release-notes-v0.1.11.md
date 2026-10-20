> **Historical note. Written 2026-10-01 (the file's date in the cloud project folder, `release-notes-v0.1.11.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

## BlocklandReImagined v0.1.11

### New
- Classic Add-Ons come with the game, credited to their authors: the Duplicator, New Duplicator, Fill Can, Grapple Rope, HookShot, Player Throwing, Trench Digging, Slayer and Capture the Flag, both Sniper Rifles, the Butterfly Knife, the HE Grenade, Emote Critical and the Stunt Plane. Kai's Tier+Tactical weapon packs and the Adventure Pack are included too. Some start turned off; turn any of them on in Add-Ons.
- Your own classic Add-Ons: drop them in the Add-Ons folder and they convert by themselves, like .bls saves.
- Slayer and Capture the Flag: teams, rounds, flags, spectating and the end of round report. Server-wide Slayer settings are in the host's Admin menu under Add-On Settings.
- Server settings: Add-Ons can now have host settings in the Admin menu. New Duplicator's settings are there.
- Environment window for admins: sky, light, fog, vignette and a day and night cycle, changed live.
- Portals come in bigger sizes that cars, Steel Balls and the Stunt Plane fit through, and a matching big Mirror. Shots, particles, cameras, the Gravity Gun and bots all go through portals.
- Copies: Duplicator copies keep brick names, lights, emitters, items, sounds, vehicles and events, and other players see your copy box.
- Change Map shows each map's own picture.
- The lag icon shows when a server you joined goes quiet.
- Add-On check: the game warns clearly when an Add-On is missing a piece, instead of failing silently.

### Fixes
- Bundled Add-Ons that need rules switch them on by themselves. HookShot, Grapple Rope, Fill Can and Trench Digging work out of the box, and the rules no longer show as separate entries.
- HookShot is blue again, and grapple ropes no longer snap back on your screen.
- The Fill Can paints bricks and vehicles, and stays in your hand when you pick a colour.
- The wrench works on Add-On bricks, so you can link Portals again.
- Duplicator messages no longer show raw colour codes like \c4.
- HE Grenade has the right look, fireball and explosion sound.
- The Sniper Rifle fires from its muzzle, and Add-On items that share a name all load.
- Bedroom Dark: breaking the bulb puts the light out.
- Destroyed vehicles turn black and lose their tires, like v20.
- The 1 key always selects your first brick.
- Joining or hosting no longer stalls when the PC is busy.
- Smoother play on big servers: other players' movement uses less bandwidth, and hosting runs the server on its own threads.

## Download

| System | Download |
|---|---|
| Windows 10 or 11 | `BlocklandReImagined.exe`, or `BlocklandReImagined-v0.1.11-windows.zip` |
| macOS (Apple silicon) | `BlocklandReImagined-v0.1.11-alpha-macos.zip` |
| Linux (x86-64) | `BlocklandReImagined-v0.1.11-alpha-linux.zip` |

Install steps are the same as for [v0.1.0](https://github.com/MaxHastings/BlocklandReImagined/releases/tag/v0.1.0-alpha).
