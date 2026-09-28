# Blockland ReImagined: features

Blockland ReImagined is Blockland v20 rebuilt from scratch. The goal is
v20 as you remember it, with its own art, music and sounds, plus Add-Ons
that can go much further than v20's ever could. This page says what is
done, what is partly done, and what is still missing. It was checked
against the game's own code and test reports on 2026-09-28.

## v20: what's done

**Menus.** Start Game, Join Game, Player (avatar), Options, About, Credits,
the Tutorial and F1 help with v20's eight help pages. The screens are v20's
own layouts.

**Maps.** All 14 v20 maps: Bedroom, Bedroom - Dark, Construct, Destruct,
Halloween Slate, Kitchen, Kitchen - Dark, Skylands, Slate, Slate Desert,
Slate Sea, Slate Storm, The Slopes and the Tutorial. Weather, water and
day lighting come with them.

**Playing.** Walking, jumping, crouching and jetting with v20's speeds,
jump and camera numbers. Falling damage, first and third person, free
look, zoom, the light, `/suicide`, and every emote (`/sit`, `/love`,
`/hate`, `/alarm`, `/confusion`, `/bsd`, `/hug`, `/wtf`, `/zombie`).

**Weapons and items.** All 21 stock items fire, sound, hit and explode as
v20's data says, checked item by item. Explosions throw debris, landings
shake the camera, arrows stick, and the pirate cannon shows its power
meter.

**Vehicles.** Jeep, Tank, Horse, Magic Carpet, Rowboat, Ball, Skis and
the Pirate Cannon: seats, weapons, tire spray, splashes, burning wrecks,
respawn and recolouring from a Vehicle Spawn brick. Steering follows
v20's strafe steering and auto-return.

**Building.** The brick selector with tabs, favourites and the brick cart;
the ghost brick with shift, super shift, rotate, plant and undo; plant
errors with their icons and sounds; paint, the seven colour FX and two
shape FX cans; the printer and prints; the hammer and wrench; build
macros. Weapons damage bricks as in v20: outside mini-games a rocket
knocks bricks out for 30 seconds, and only the hammer, the wand and undo
remove them for good.

**Wrench and events.** Names, lights, emitters, items, respawn,
raycasting, collision and rendering. The events dialog with all 16 inputs
and all 65 outputs. Demo Pong in Bedroom plays.

**Mini-games.** Create, join, leave, invite, remove, reset and end, with
all 21 rule settings, ten favourite presets, scores in the player list,
kill messages and respawn.

**Bots.** Blockhead Bots from Vehicle Spawn bricks. They wander near their
brick and fight inside the owner's mini-game.

**Save and load.** Builds save with their description, events and
ownership, load per map, and sort by name or date. Loading a save made
with a different colour set asks how to load its colours, as v20 did.

**Avatar.** Every part, face, decal, pack, hat and accent, part colours,
the colour picker, and ten favourites.

**Hosting and admin.** Single player, LAN and Internet games. Server name,
player limit, admin and super admin passwords, Advanced Config (quotas,
the public domain timeout and the chat filter apply) and Music Files. The
Admin menu: kick, ban, unban, clear a player's bricks, change
map, and server settings (brick limit, bricks per second, chat length,
build distance, random brick colour with the next colour on the ghost,
falling damage, vehicle limits). The
wand, the F7/F8 admin camera, and `/fetch`, `/find`, `/warp`,
`/timescale`, `/spy`, `/ret`, `/realbrickcount`, `/cancelallevents`,
`/clearbots` and the vehicle resets. `/brickcount` and `/clearinventory`
for everyone.

**Chat and players.** Say and team chat, the talking indicator, name tags,
centre and bottom prints, the player list with trust and ignore, and the
console (`~`).

**Options and keys.** Resolution, fullscreen, VSync, shadows, draw
distance, texture filtering, precipitation, chat settings, HUD toggles,
volumes, music and sound toggles, mouse and keyboard settings, invert
mouse in vehicles, Censor Chat, Press Up to Repeat Chat, the ghost brick
colour and flash settings, Auto Light, the two steering settings, Render
Items and Jets in first person, and all 81 of v20's remappable actions
with its default keys. Windows move, resize, minimize and maximize where
v20's did, and the F1 help pages keep v20's headings and coloured keys.

## v20: partly done

- **Graphics quality.** v20's separate lighting, particle, texture,
  physics and brick FX radios are replaced by Low, Medium and High presets.
- **Event outputs.** Projectile outputs on delayed event rows aren't
  applied yet. The immediate ones work.
- **Music Files.** The host's choice limits music bricks, but a player
  joining still sees every track listed in the wrench.
- **Saving on someone else's server.** Guests can't save the host's
  world. v20's warning text for this isn't shown.
- **Screenshots** are always PNG.

## v20: still missing

- **Options:** Render My Player.
- **Join passwords** are hidden for now.

Left out on purpose: Blockland account keys and BL_IDs (players are known
by a key the game makes for them), Fast Load, and Torque's debug commands
and render modes.

## Beyond v20

- **Easy hosting.** The game asks your router to open its port, tells you
  in chat whether friends can reach you, and copies an invite to paste
  into Connect to IP. It can fix the Windows firewall for you. LAN games
  appear by themselves, with favourites and recent servers in the Join
  list.
- **Safety nets.** Autosave, a prompt before leaving unsaved work, a
  crash report you can send, a clear message instead of a silent exit,
  and rejoining keeps your bricks.
- **First start.** Picks graphics for your hardware, offers the Tutorial,
  and asks your name.
- **Settings v20 didn't have.** Graphics presets, a frame rate cap, field
  of view, anti-aliasing, brick shadows, UI size, colour-vision modes,
  sound captions, a music volume, mute in the background, Toggle Crouch,
  and mouse buttons 4 and 5.
- **Building extras.** Brick search in the selector, a red ghost before a
  plant that would fail, and the Duplicator (type `/dup`) to copy whole
  builds.
- **Gamepad** while playing.
- **Performance overlay** (F3) and net graph (Ctrl+N).
- **Knocked-out bricks** can be pushed around by players, vehicles and
  shots. This is only for show and never affects play.

## Add-Ons

**Making Add-Ons.** New Add-Ons can add:

- game rules: points, rounds, chat commands, written in a small script
  language and run by the host
- HUD panels for those rules
- weapons, described as data
- game modes in Start Game
- generated worlds, creatures and playable bodies
- tools that act where you click them, like the Duplicator

A guide, samples, and tools that check an Add-On and try its rules
without opening the game come with the source code.

**Sharing.** Players joining a server download the Add-Ons it runs that
they don't have, or have in another version, and go straight into the
game. Their own Add-Ons the server doesn't run sit that game out.

**Add-On code on your PC.** An Add-On can also run its own drawing, sound
and input code on players' PCs, for things like custom visuals. It runs
in a sandbox with limits on what it can touch and how much it can use.
Before any server's code runs, the game asks "Trust and join" or "Leave",
and asks again if that code changes. Forget Trust on the Add-Ons screen
takes it back.

**Not yet:** writing new bricks directly (for now, you make a small
v20-style brick Add-On and import it), and drawing block faces. Blocks
load, save and play, but aren't drawn yet.

## Old v20 Add-Ons

Put an old Add-On (`.zip`) in `content\Add-Ons`, open **Start Game >
Add-Ons**, pick it and press **Import**. The importer turns it into a new
Add-On, which starts off until you turn it on:

- **Bricks, weapons, vehicles, sounds, textures and models come across.**
  v20 described these as datablocks, and the importer turns them into the
  game's own data.
- **Scripts never run and don't carry over.** Custom behaviour written in
  v20's script language (special weapon firing, bot frameworks, custom
  commands, GUIs) is left out. The import report, `IMPORT-REPORT.md` in
  the new Add-On, lists what came across and what didn't.
- **Custom behaviour needs a port.** To get it back, someone rewrites it
  as a native Add-On rule, using the report as the list of what to write.

In a test over 255 old Add-Ons, 253 imported. 31 came across completely
and 81 came across with gaps. The other 141 had nothing to convert
because they were only scripts, maps or GUIs. Old maps (`.mis`) and
saves (`.bls`) inside Add-Ons aren't imported.
