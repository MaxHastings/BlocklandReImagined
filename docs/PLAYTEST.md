# Maxwell's first building playtest

This build is for testing the core building experience. It is **not the complete
vanilla alpha**. Read `KNOWN-ISSUES.md` for the unfinished systems.

## Start

1. Keep the package folder intact and run `Launch.cmd`. No Rust/compiler or
   original v20 installation is required. Keep it in a writable local folder.
2. Complete the first-run mouse/keyboard choice. Choose the standard keyboard
   scheme if you have a numpad; otherwise choose laptop controls.
3. Start a single-player game in **Bedroom**. Leave the server join password
   blank. Then try Kitchen, Slopes and Slate in separate sessions.

Your settings, native identity and saves live under this package's `user-state/`.
Launch logs live under `logs/`. Preserve both when reporting a problem; do not
share `client.identity` or the entire user-state directory publicly.

## Test the experience

- **Menus/input:** change sensitivity, resolution and audio volume; close and
  reopen Options. Check typing in chat or dialogs doesn't move the player, and
  closing a menu restores mouse look without stuck movement/jetting.
- **Movement:** walk, jump, crouch and jet around the map. Test floors, steps,
  walls and corners. Compare first/third-person view, held zoom and free look.
  Tell me specifically what feels wrong compared with v20.
- **Building:** choose several ordinary bricks, slopes and a printable brick.
  Move/rotate the ghost, hold movement keys to repeat, plant, cancel and undo.
  Build a small connected structure; check ghost placement against the result.
- **Tools:** paint the build; hammer a brick; apply a print; wrench a name,
  collision/render setting, light and emitter. On a spare brick, try the supported
  `onActivate → Self → setColor` event. Full vanilla events are not ready.
- **Persistence:** save the build with events and ownership. Leave the session,
  start the same map, then load it. Verify positions, paint, prints and properties.
  Try a small original reference save before stress-testing a large one. Loading
  appends bricks; use an empty session when checking a clean round trip.
- **Session lifecycle:** disconnect during loading, start again, change maps,
  leave/re-enter a session, quit and relaunch. Verify preferences and saves remain.

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

Options → Controls shows/remaps your selected scheme. Laptop scheme uses
I/K/J/L, P/semicolon, period/comma, O/U, Enter and slash for brick controls.

## Optional two-player check

1. Use the same package on both PCs. Host a LAN game, leave join password blank,
   and allow the game through Windows Firewall on your private network if asked.
2. Give the second player the host's `user-state/host-certificate.der` and LAN
   address, for example `192.168.1.20:28000`. Share only that public certificate.
3. On the joining PC, copy that public certificate to the package's
   `user-state/host-certificate.der`. Run `Trust Host.cmd` and enter the exact
   address. Then use the game's direct-IP join with the same address.
   Repeat certificate import after the host restarts; persistent server
   certificates and a normal in-game discovery/trust flow remain unfinished.
4. Build and chat from both PCs. Join after a small build already exists and
   check it arrives correctly. Check unauthorized edits to the other player's
   bricks are rejected. Full trust-management UI is not ready.

## Send back

For each issue: map/save, steps, expected v20 behavior, actual behavior, whether
it repeats, and the latest file from `logs/`. Screenshots/video are useful when
you choose to take them. Separate crashes/blockers from movement/building feel
and visual differences. Your actual mouse/keyboard playtest is the missing
evidence; the automated checks did not operate your desktop or listen to audio.
