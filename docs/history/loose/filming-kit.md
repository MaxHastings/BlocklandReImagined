> **Historical note. Written 2026-10-05 (the file's date in the cloud project folder, `filming-kit.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Filming kit for the v0.2.4 video

Checked against a release-style build of the v0.2.4 work with your own
settings and saves. Where a step names a key, it is the default bind.

## Before you hit record

1. Launch the game and leave it on the main menu for about 30 seconds the
   first time. The world shaders compile in the background (about 20 s on
   an idle PC, minutes if something else is using the CPU). In v0.2.4 the
   loading screen waits for them instead of freezing, but the wait is
   still there on the first map.
2. Close anything heavy (builds, other games). The shader compile and the
   recorder both want the CPU.
3. Your settings now: UI Size 200%, Lighting Dynamic, Max FPS unlimited,
   windowed 1280x720. For the video, try UI Size 100-150% (chat text is
   very large at 200% in 1080p), fullscreen at your recording resolution,
   and Max FPS at 60 or 120 so frame pacing matches the recording.
4. Add-Ons (all ship with the game; your v0.2.3 install already has these
   on): Blockhead Bot, Ragdoll, Gravity Gun (+ Tool, + Effects), Steel Ball
   (+ Kit, + Effects), Portal, Mirror, Sniper Rifle, HE Grenade, Stunt
   Plane, New Duplicator, and MiniNuke once its Add-On lands.

## Beat by beat

| Beat | How |
|---|---|
| Bedroom | Start Game > Bedroom. |
| Brick search | B opens the selector; type in Search. |
| Spray paint and FX | E for the spray can; the FX row is at the end of the paint list (pearl, chrome, glow, blink, swirl, rainbow). |
| Printer, hammer, wrench | Q for tools: hammer, wrench, printer. |
| Light, emitter, item | Wrench a brick: Light, Emitter, Item menus, then Send. |
| Rock music | Plant a Music brick, wrench it, pick Rock, Send. |
| Duplicator | Type /dup (the New Duplicator). Click the build to select it, press a brick key (or Plant Brick) to pick it up as a ghost, move it, Plant Brick to place. Cancel Brick leaves plant mode. |
| Avatar Randomize | Esc > Player, Randomize!, then Done. |
| ACM City | Load Bricks > Bedroom > ACM City (16,478 bricks, loads in under a second). |
| Jeep, Horse, Tank | Plant a Vehicle Spawn brick, wrench it, pick the vehicle. Jump onto it to get on, right-click to get off. |
| Mini-game, rocket launcher | Esc > Minigames > Create; put the Rocket Launcher, Sword, Hammer, Spear and Sniper Rifle in the loadout. Brick damage on. |
| Bots | Vehicle Spawn brick > Blockhead Bot, inside the mini-game. They attack you, but bots from your own bricks never fight each other (as in v20). For bots fighting bots, use Slayer's "Bots (free-for-all)" or its team bots. |
| Sniper zoom | F while holding it. |
| HE Grenade | In a mini-game loadout (it shows v20's rocket sphere for its blast, as v20 does). |
| Gravity Gun | Outside a mini-game it is in your tools; in one, add it to the loadout. |
| Ragdolls | Ragdoll Add-On on; any kill. |
| Steel ball | Vehicle Spawn brick > Steel Ball. |
| Admin camera | F8 flies the camera. F7 drops you where the camera is; /ret returns to your body where you left it. |
| Pong and events | Load Bricks > Bedroom > Demo Pong. Wrench a brick > Events for the editor. |
| MoTE Mansion garden | Load Bricks > Bedroom > Mansion (the one by Pload). |
| Day to night | Ctrl+A > Environment: turn on the day/night cycle and set Day Length to 60 s so the change is visible on camera; Time of Day 0.6 starts it in late afternoon. |
| Kitchen | Ctrl+A > Change Map > Kitchen, same environment settings. L is your player light. |
| Slopes | Change Map > Slopes. Skis in a mini-game loadout; Stunt Plane on a Vehicle Spawn brick. |
| Skylands | Change Map > Skylands. |
| Badspot Block Party | Change Map > Slate, Load Bricks > Slate > Badspot's Block Party1 (68,746 bricks, about 6 s). Bigger ones: Badspot's River Party (95k), Block Party Christmas 09 (75k). |
| F3 | F3 toggles the performance overlay; Ctrl+F3 saves a capture. |
| Credits | Host on LAN; the other two use Join Server. F2 shows the player list. Emotes: /love /hug /hate /alarm /confusion /wtf. |

## Things that look rough on camera

- The first map after launch shows the loading screen for about 20 s while
  shaders compile (see "Before you hit record").
- A new map's first frame takes 0.2-0.4 s to appear after its loading
  screen.
- With F3 open, the overlay sits over the "Q = tools" hint in the top
  right.
