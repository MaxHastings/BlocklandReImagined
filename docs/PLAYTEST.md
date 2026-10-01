# Blockland ReImagined — alpha playtest

This build brings building, deathmatch minigames, vehicles, bots and
multiplayer over LAN or the internet together. Start with `TESTER-GUIDE.md`:
its Install section covers setup and the "Windows protected your PC"
warning, and Playing together covers LAN and internet games. `FEATURES.md`
says what is done and what isn't, and `KNOWN-ISSUES.md` lists what is still
unfinished. Slate and Bedroom are good first maps.

## Version and updates

The main menu's bottom corner shows the version (for example
`2026-09-28-a13 (1a2b3c4d5)`); include it when reporting a problem. Once per
start the game asks the project's GitHub Releases page whether a newer version
exists and, if so, offers to open that page. It never downloads or installs
anything and says nothing when offline. Turn it off with Options > Advanced >
"Check for new versions".

## Things to try

- **Movement:** walk, jump, crouch, jet (hold right mouse), crouch-jet for the
  forward boost. Your own movement is predicted locally, so it should feel
  immediate and smooth; tell me anything that feels different from v20.
- **Building:** brick selector (B), ghost moves with the numpad, plant, rotate,
  undo, paint (E), hammer, printer, wrench (names, lights, emitters, items,
  collision/rendering). Blocked placements show the original plant-error icon.
- **Duplicator** (an Add-On, on by default): type `/dup` (or `/duplicator`) for the blue
  Duplicator, then click the bottom brick of a build. It copies that brick
  and everything built on it (up to 2000 bricks) and shows the copy as a
  ghost over the original. Move and turn it with the numpad like a brick
  ghost (super shift moves it by its own size), plant it with Numpad Enter,
  and Ctrl+Z takes the whole copy back. A copy that cannot all be planted
  (overlap, floating, someone else's bricks, brick limit) plants nothing.
- **Deathmatch:** Escape → Mini-Games → Create. The default rules give everyone
  a gun and rocket launcher. Shoot, fall, die, watch the respawn countdown and
  click to respawn. Kill messages appear in chat. The player list (F2) shows scores.
- **Vehicles:** plant a Vehicle Spawn brick (Special tab), wrench it and pick a
  vehicle (Jeep, Tank, Horse, Magic Carpet, Rowboat, Ball…). Walk into it to get
  in; W/S throttle, A/D steer, Space gets out (Shift on the horse), period and
  comma change seats, left mouse fires vehicle weapons. Tick "Recolor Vehicle" to paint
  it with the brick color, and use `< Respawn >` to reset it.
- **Bots:** pick "Blockhead Bot" in a Vehicle Spawn brick. Bots wander near their
  brick; inside your minigame they fight you.
- **Music:** plant a Music brick and choose a loop in its wrench.
- **Other keys:** light (L), suicide (Ctrl+K), team chat (Y), screenshots
  (Ctrl+P, Shift+P without the HUD), net graph (Ctrl+N), build macro record /
  play (Ctrl+Numpad 0 / Ctrl+Numpad Enter), admin free camera (F8, fly with
  WASD, Space/Shift up/down) and drop yourself there (F7). Emotes have no default
  key: bind them in Options → Controls or type `/love`, `/hate`, `/alarm`,
  `/confusion` or `/sit`. `/suicide` and `/light` work in chat too.
- **Save/load:** save a build (with events and ownership), leave, load it back.

## Playing with a friend

See Playing together in `TESTER-GUIDE.md`. Then try building together, a
minigame deathmatch, riding one jeep together and a late join into an
existing build.

## If the game runs slowly

The first start picks Low, Medium or High graphics from your GPU (Low for
software rendering, Medium for graphics built into the processor, High for a
graphics card). The choice and your GPU's name are in the session log. On a
weak PC, try these in order:

1. Options > Graphics > Shadow Quality Minimum (turns sun shadows off), and
   turn Anti-Aliasing and Brick Shadows off. These cost the most.
2. Pick a smaller resolution, or play windowed at 1280x720.
3. Turn precipitation off (rain and snow maps).
4. Laptops: plug in and set Windows to Best performance. On a laptop with two
   GPUs, choose High performance for `bri-client.exe` in Settings > System >
   Display > Graphics.
5. Update the graphics driver from Intel, AMD or NVIDIA.
6. Start on a small map (Slate, Bedroom) before big builds.

Every minute of play the session log records frame times ("Frame times over
60 s: ... fps average, median ... ms, 1% slowest ... ms"). Send the session
log with any "it's slow" report; the console's `stats` command shows the
current numbers.

## Default controls reminder

| Action | Standard keyboard / wheel mouse |
|---|---|
| Move / jump / crouch | WASD / Space / Left Shift |
| Jet / zoom / free look | Hold right mouse / F / Z |
| First/third person / pause / chat | Tab / Escape / T |
| Brick selector / bricks / tools / paint | B / 1 / Q / E |
| Inventory selection / use | Mouse wheel / left mouse |
| Ghost horizontal movement | Numpad 8, 2, 4, 6 |
| Up/down a brick; up/down a plate | Numpad +, 5; Numpad 3, 1 |
| Rotate / plant / cancel / plant undo | Numpad 9 or 7 / Numpad Enter / Numpad 0 / Ctrl+Z |

## Send back

See When something breaks in `TESTER-GUIDE.md`. Separate crashes and
blockers from feel and visual differences.
