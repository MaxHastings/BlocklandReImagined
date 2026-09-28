# Blockland ReImagined — alpha playtest

This build brings building, deathmatch minigames, vehicles, bots and LAN
multiplayer together. Read `KNOWN-ISSUES.md` for what is still unfinished.

## Start

1. Keep the package folder intact in a writable folder and run `Launch.cmd`.
   No Rust, compiler or original v20 installation is needed.
2. Choose your mouse/keyboard scheme on first run (standard if you have a
   numpad, laptop otherwise).
3. Start a single-player game. Slate and Bedroom are good first maps.

Settings, identity, saves and screenshots live under `user-state/`; logs live
under `logs/`. Keep both when reporting a problem. Do not share
`client.identity` publicly.

## Things to try

- **Movement:** walk, jump, crouch, jet (hold right mouse), crouch-jet for the
  forward boost. Your own movement is predicted locally, so it should feel
  immediate and smooth; tell me anything that feels different from v20.
- **Building:** brick selector (B), ghost moves with the numpad, plant, rotate,
  undo, paint (E), hammer, printer, wrench (names, lights, emitters, items,
  collision/rendering). Blocked placements show the original plant-error icon.
- **Deathmatch:** Escape → Mini-Games → Create. The default rules give everyone
  a gun and rocket launcher. Shoot, fall, die, watch the respawn countdown and
  click to respawn. Kill messages appear in chat. Tab shows scores.
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

1. The host starts a LAN game (Start Game → server type LAN). Allow the game
   through Windows Firewall on private networks when asked.
2. The other player opens Join Server; LAN games appear in the list, and so do
   servers you joined before. Connect to IP takes an address like
   `192.168.1.20`, `192.168.1.20:28000` or a host name such as
   `play.example.com`, and remembers the last one typed. Only UDP port 28000
   (or the port in the address) needs to be open on the host. The first join
   trusts the host's identity and remembers it.
3. Try building together, a minigame deathmatch, riding one jeep together and a
   late join into an existing build.

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

For each issue: map, what you did, what v20 would do, what happened, whether it
repeats, and the latest file from `logs/` (after a crash, also the
newest `crash-*.txt` and any `crash-*.dmp`). Screenshots help (they save under
`user-state/screenshots/`). Separate crashes and blockers from feel and visual
differences.
