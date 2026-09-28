# Blockland ReImagined — alpha playtest

This build brings building, deathmatch minigames, vehicles, bots and LAN
multiplayer together. Start with `TESTER-GUIDE.md` (install, playing
together, what to send). `FEATURES.md` says what is done and what isn't, and
`KNOWN-ISSUES.md` lists what is still unfinished.

## Start

1. Keep the game folder intact in a writable folder and run `Launch.cmd`.
   No Rust, compiler or original v20 installation is needed.
2. Choose your mouse/keyboard scheme on first run (standard if you have a
   numpad, laptop otherwise).
3. Start a single-player game. Slate and Bedroom are good first maps.

Settings, identity, saves and screenshots live under `user-state/`; logs live
under `logs/`. Keep both when reporting a problem. Do not share
`client.identity` publicly.

### "Windows protected your PC"

The game is not signed with a paid certificate yet, so Windows SmartScreen
may stop it the first time. Click **More info**, check the app name is
`bri-client.exe` or `Launch.cmd`, then click **Run anyway**. Windows remembers
the choice for this copy. If the download itself is blocked, right-click the
zip, choose **Properties**, tick **Unblock** and extract it again.

### Version and updates

The main menu's bottom corner shows the version (for example
`2026-09-28-a13 (1a2b3c4d5)`); include it when reporting a problem. Once per
start the game asks the project's GitHub Releases page whether a newer version
exists and, if so, offers to open that page. It never downloads or installs
anything and says nothing when offline. Turn it off with Options > Advanced >
"Check for new versions". To update, extract the new version to a new folder;
copy `user-state/` across to keep settings, saves and identity.

## Things to try

- **Movement:** walk, jump, crouch, jet (hold right mouse), crouch-jet for the
  forward boost. Your own movement is predicted locally, so it should feel
  immediate and smooth; tell me anything that feels different from v20.
- **Building:** brick selector (B), ghost moves with the numpad, plant, rotate,
  undo, paint (E), hammer, printer, wrench (names, lights, emitters, items,
  collision/rendering). Blocked placements show the original plant-error icon.
- **Duplicator** (an Add-On, on by default): type `/dup` for the blue
  Duplicator, then click the bottom brick of a build. It copies that brick
  and everything built on it (up to 2000 bricks) and shows the copy as a
  ghost over the original. Move and turn it with the numpad like a brick
  ghost (super shift moves it by its own size), plant it with Numpad Enter,
  and Ctrl+Z takes the whole copy back. A copy that cannot all be planted
  (overlap, floating, someone else's bricks, brick limit) plants nothing.
- **Gravity Gun** (an Add-On, on by default): everyone gets one; `/gravitygun`
  puts it in your hand. Right click grabs what you aim at (players,
  vehicles, Steel Balls) and right click again drops it; left click punts;
  hold left click to charge a throw and release to throw. Heavy things lag
  and sag in the beam. In a minigame, a thrown vehicle that lands on
  someone kills them and the kill is yours; outside minigames you can only
  move players and vehicles whose owners trust you, and nobody is hurt.
- **Steel Ball** (an Add-On, on by default): everyone gets the steel ball
  item; `/steelball` puts it in your hand. Left click rolls a heavy
  polished ball out, right click hurls one; you keep three, and
  `/clearballs` puts them away. It rolls with real weight, bowls players
  over and shoves vehicles aside; in a minigame it hurts, and a hard hit
  knocks bricks out like a rocket. It is also a seatless vehicle on the
  Vehicle Spawn brick's list. Try throwing one with the Gravity Gun.
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

## Playing with a friend (LAN)

1. The host starts a LAN game (Start Game → server type LAN). If Windows asks
   about the firewall, choose Allow; if friends would still be blocked, the
   game offers to fix it (one Windows permission prompt).
2. The other player opens Join Server; LAN games appear in the list, and so do
   servers you joined before or starred with Favorite. Connect to IP takes an
   address like `192.168.1.20`, `192.168.1.20:28000`, a host name such as
   `play.example.com`, or an invite (`bri://…`), and remembers the last one
   typed. Only UDP port 28000 (or the port in the address) needs to be open on
   the host. The first join trusts the host's identity and remembers it.

## Playing with a friend over the internet

1. The host starts an Internet game. Within about ten seconds the chat says
   whether friends can reach you, and what to change if not. When they can,
   your invite is on the clipboard; type `/invite` to copy it again.
2. The friend pastes the invite into Join Server → Connect to IP.
3. Try building together, a minigame deathmatch, riding one jeep together and a
   late join into an existing build.

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

For each issue: the version from the main menu, map, what you did, what v20
would do, what happened, whether it repeats, and the latest file from `logs/`.
After a crash the next start names the crash files (`crash-*.txt` and, for a
native crash, `crash-*.dmp`) and offers to open the folder; send those too.
Nothing is uploaded automatically. Screenshots help (they save under
`user-state/screenshots/`). Separate crashes and blockers from feel and visual
differences.
