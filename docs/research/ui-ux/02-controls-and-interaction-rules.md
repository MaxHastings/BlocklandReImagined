# 02 — Controls, defaults and interaction rules

Evidence levels and citation keys are defined in [README.md](README.md). The
complete machine-extracted bind table, with enclosing conditions, is
`data/default-binds-apply.tsv`.

## 1. Default key and mouse bindings

v20 has **no hard-coded default keymap on disk**. The action map starts empty
(`new ActionMap(moveMap)`@c:20672). It is built by `defaultControlsGui::apply`
from the first-run choice of mouse type and keyboard type, then saved to
`config/client/config.cs`. Global binds are separate (`GlobalActionMap`@c:21014). **[V]**

### 1.1 Always bound (all schemes)

| Key | Command | Behaviour | Source |
|---|---|---|---|
| Esc | escapeMenu.toggle() | escape menu (disconnects on LoadingGui) | c:15313 |
| W / S / A / D | moveforward / movebackward / moveleft / moveright | analog-style: value × `$RunMultiplier` | c:15314–15317 |
| Space | Jump | trigger 2 (also trigger 4 = jet when "Jump/Jet Combo") | c:15318, `Jump`@c:20801 |
| Left Shift | Crouch | trigger 3, held | c:15319 |
| C | Walk | **held**: movement × 0.4 | c:15320, `Walk`@c:20811 |
| F | toggleZoom | **held**: FOV = `$Pref::player::CurrentFOV` (default 45) | c:15321, `toggleZoom`@c:20871 |
| Z | toggleFreeLook | **held** free look | c:15322 |
| Tab | toggleFirstPerson | toggle 1st/3rd person (smooth unless "Fast switch") | c:15323, `toggleFirstPerson`@c:20897 |
| F8 / F7 | dropCameraAtPlayer / dropPlayerAtCamera | camera ↔ player (admin/free-cam) | c:15324–15325 |
| T / Y | GlobalChat / TeamChat | open chat input | c:15326–15327 |
| PageUp / PageDown | chat scroll | 200 ms first repeat, then 50 ms | c:15328–15329, `PageUpNewChatHud`@c:5292 |
| M | ToggleCursor | mouse cursor for clicking chat links (not in single player) | c:15330, `ToggleCursor`@c:5360 |
| 1 | useBricks | brick mode, current slot (**not** "first slot") | c:15331 |
| 2–9, 0 | useSecondSlot … useTenthSlot | select brick slot 2–10 | c:15335–15343 |
| Q | useTools | tool mode / cycle | c:15332 |
| E | useSprayCan | paint mode / next paint column | c:15333 |
| Ctrl+W | dropTool | throw current tool | c:15334 |
| Ctrl+Z (Cmd+Z mac) | undoBrick | undo last plant/paint/print | c:15346 |
| Left Alt (Right Option mac) | toggleSuperShift | super-shift (see §4.4) | c:15354 |
| Ctrl+A | openAdminWindow | admin GUI or login | c:15355 |
| Ctrl+O | openOptionsWindow | options | c:15356 |
| Ctrl+P / Shift+P / Shift+Ctrl+P | screenshot / HUD-less / DOF screenshot | | c:15357–15360 |
| Ctrl+K | Suicide | | c:15358 |
| F2 | showPlayerList | toggle player list | c:15372 |
| Ctrl+N | toggleNetGraph | | c:15373 |
| Mouse X / Y | yaw / pitch | sensitivity × FOV/90 × 0.005 | c:15374–15375 |
| Mouse Left | mouseFire | trigger 0: use tool/brick/fire; **activate when empty-handed** | c:15376 |
| B | openBSD | brick selector (B again = DONE) | c:15377 |
| F5 | ToggleShapeNameHud | toggles player names **and crosshair** | c:15378 |
| ~ | toggleConsole | global | `toggleConsole`@c:21014 |
| Alt+Enter | toggleFullScreen() | global | c:21015 |
| F1 | contextHelp() | global (help dialog) | c:21016 |

### 1.2 Keyboard scheme: Standard (numpad, the default) vs Laptop

| Action | Standard keyboard | Laptop keyboard |
|---|---|---|
| Shift brick away / towards | Numpad 8 / Numpad 2 | I / K |
| Shift brick left / right | Numpad 4 / Numpad 6 | J / L |
| Shift brick up (1 brick) | **Numpad +** (bound as "+" on Windows, "numpadadd" on mac) | P |
| Shift brick down (1 brick) | Numpad 5 | ; |
| Shift up / down 1 plate | Numpad 3 / Numpad 1 | . / , |
| Rotate CW / CCW | Numpad 9 / Numpad 7 | O / U |
| Plant brick | Numpad Enter | Enter |
| Cancel brick | Numpad 0 | / |
| Super-shift (explicit) | Alt + numpad 8/2/4/6/+/5 | Alt + I/K/J/L/P/; |
| Build macro record / play | Ctrl+Numpad 0 / Ctrl+Numpad Enter | Ctrl+/ / Ctrl+Enter |
| Light | L | [ |
| Next / prev vehicle seat | . / , | Ctrl+. / Ctrl+, |

Source: `%keyboard == 0`@c:15379 through c:15460. **[V]**

### 1.3 Mouse scheme

| Scheme | Right mouse | Wheel | Arrow keys |
|---|---|---|---|
| One button | — (jump also jets: `$pref::Input::noobjet = 1`) | — | Up/Down/Left/Right → inventory |
| Two button | **Jet** (trigger 4) | — | inventory |
| Two button + wheel (**default**) | **Jet** | **scrollInventory** | not bound (a stray Ctrl+E → invLeft bind) |
| Tilt wheel (hidden option) | Jet | scrollInventory | as above |

Source: `%mouse == 0`@c:15463 through c:15488. **[V]** The "Ctrl+E → invLeft"
bind in wheel schemes looks accidental. Preserve it only if cheap **[A]**.

### 1.4 Stock remap list (Options → Controls)

`$RemapDivision` / `$RemapName` / `$RemapCmd`@c:3024–3265, in display order:

- **Movement**: Forward, Backward, Strafe Left, Strafe Right, Jump, Crouch, Walk, Jet
- **View**: Turn Left, Turn Right, Look Up, Look Down, Toggle Zoom, Free Look,
  Switch 1st/3rd, Drop Camera at Player, Drop Player at Camera
- **Action**: Fire Weapon/Tool, Suicide, Next Vehicle Seat, Prev Vehicle Seat
- **Communication**: Global Chat, Team Chat, Chat Hud PageUp, Chat Hud PageDown
- **Gui**: Toggle Cursor, Open Admin Window, Open Options Window, Show Player
  List, Toggle NetGraph, Toggle Player Names / Crosshair
- **Tools / Inventory**: Use Bricks, Use Tools, Use Spray Can, Use Light, Drop
  Tool, Use 1st…10th Slot, Inventory Up/Down/Left/Right
- **Building**: Open Brick Selector, Plant Brick, Undo Brick, Cancel Brick,
  Shift Brick Away/Towards/Left/Right/Up/Down, Shift Brick Up 1/3, Shift Brick
  Down 1/3, Rotate Brick CW/CCW, Toggle Super Shift, Super Shift Brick
  Away/Towards/Left/Right/Up/Down, Toggle Build Macro Recording, Playback
  Build Macro
- **Recording**: Take Hud Screenshot, Take Screenshot, Take DOF Screenshot
- **Emotes**: Sit, Love, Hate, Confusion, Alarm (unbound by default)

Remap interaction (`OptRemapInputCtrl::onInputEvent`@c:3460): select a row and
press a key. A conflicting bind asks to replace it. "Remap All" walks every
row. "Clear All" unbinds everything. "Defaults >>" reopens the first-run
dialog. **[V]**

### 1.5 Differences in Maxwell's install

The B4v21 install changes stock client defaults (full table in
`data/client-defaults-stock-vs-installed.tsv`). Control-relevant differences:
`UseStrafeSteering` 1→0, `UseAutoReturnSteering` 1→0, `VehicleMouseInvert`
1→0, and added `$Pref::Input::brickRepeatTime` / `brickFirstRepeatTime`
(same 50/200 values). Maxwell's actual `config/client/config.cs` binds were
**not read** (personal configuration). If he wants his exact binds, import that
file with his consent **[M]**.

## 2. Movement and camera semantics (client side)

| Rule | Detail | Evidence |
|---|---|---|
| Walk is a multiplier | C held sets `$RunMultiplier = 0.4`, and currently held directions are updated immediately | `Walk`@c:20811 |
| Keyboard turning | Turn/Look keys use `$pref::Input::KeyboardTurnSpeed` (stock 0.5) as a rate | `turnLeft`@c:20743 |
| Mouse scale | `sens × (cameraFov/90) × 0.005` per count. The script defines `getMouseAdjustAmount` twice (0.001, then 0.005). The later one wins **[A]** (TorqueScript last definition) | `getMouseAdjustAmount`@c:20768 |
| Vehicle mouse invert | While driving (not strafe-steering, not free-looking) pitch uses `$Pref::Input::VehicleMouseInvert` (stock **1**) | `amIDrivingAVehicle`@c:20783 |
| Zoom | Hold F → FOV `$Pref::player::CurrentFOV` (default 45). Release → `$pref::Player::defaultFov` 90. Wheel while zoomed changes zoom FOV ±5 within 5–85 | `$ZoomOn`@c:4492, `CurrentFOV`@c:20861 |
| 1st/3rd person | Tab toggles `$firstPerson`, with `$cameraSpeed = 5` (smooth) or 1000 with Fast switch | `$cameraSpeed`@c:20907 |
| Jump/jet combo | One-button mice, or the "Use Jump/Jet Combo" option, make Space also trigger jet | `noobjet`@c:20803 |
| Player datablock | Standard player: `maxForwardSpeed` 7, back 4, side 6, crouch 3/2/2, `jumpForce` 12×90, `mass` 90, `cameraMaxDist` 8, default FOV 90, `canJet` 1 (Astra's domain; values for reference) | `maxForwardSpeed`@s:8764, `cameraMaxDist`@s:8744 |

## 3. Scroll-mode state machine (bricks / paint / tools)

The HUD always has exactly one mode: BRICKS=0, PAINT=1, TOOLS=2, NONE=3
(`$SCROLLMODE_BRICKS`@c:4453). **[V]**

- **Entering a mode** slides that box in and, if others hide, slides them out.
  Leaving PAINT or TOOLS sends `unUseTool`, unless the change came from an
  instant-use brick (`setScrollMode`@c:4852).
- **1 (useBricks)**: while building is disabled, shows "Building is currently
  disabled." Otherwise selects the current slot (`useBricks`@c:4380).
- **2–0 (direct slot)**: selecting an occupied slot enters BRICKS and sends
  `useInventory`. Pressing the **already active slot again deselects** it,
  unequips and returns to NONE (`directSelectInv`@c:3695). Selecting an empty
  slot walks to the next filled slot. If there are none, it uses the last
  instant-use brick. If there is none of those either, it shows
  "You don't have any bricks!\nPress B to open the brick selector." for 3 s
  (`You don't have any bricks`@c:3800). **[V]**
- **Mouse wheel** (`scrollInventory`@c:4457): ignored while more than two GUI
  layers are open (menus). While zoomed it changes zoom instead. Otherwise it
  scrolls within the current mode, skipping empty slots and wrapping. In NONE
  it enters BRICKS (or TOOLS when building is disabled). "Reverse Brick
  Scrolling" flips the direction. The script maps a negative wheel delta to +1
  (`%val < 0`@c:4484). With Torque's positive-is-wheel-up convention **[E]**,
  wheel up selects the previous slot. Confirm by playtest **[A]**.
- **Q (useTools)**: from another mode, enters TOOLS on the current or next
  filled tool slot. In TOOLS it returns to NONE (`useTools`@c:4324).
- **E (useSprayCan)**: enters PAINT. In PAINT it moves to the **next color
  column** (division), wrapping, and keeps the swatch index clamped
  (`shiftPaintColumn`@c:4590). The wheel moves within a column. Choosing a
  color sends `useSprayCan(index)` and recolors the HUD brick icons. The FX
  column sends `useFXCan(0..8)`.
- **Arrow keys** (non-wheel schemes): Up/Down scroll the current mode,
  Left/Right change the paint column in PAINT (`invLeft`@c:5608).
- **Server-forced**: `clientCmdSetActiveTool` / `SetActiveBrick` switch modes
  remotely (c:4360, c:4370).

## 4. Building interactions

### 4.1 Getting a ghost ("temp") brick
- Selecting a brick mounts the brick image in the player's hand
  (`fxDTSBrickData::onUse`@s:16413). **Clicking** fires an invisible
  `brickDeployProjectile` (60 u/s, 250 ms lifetime ⇒ about 15 units of reach)
  (`muzzleVelocity`@s:14962). Where it hits, the ghost brick is created or
  moved (`brickDeployProjectile::OnCollision`@s:15010). **[V]** Reach and
  raycast behaviour follow engine projectile simulation **[E/A]**.
- Ghost orientation comes from the player's facing (`getAngleIDFromPlayer` +
  `orientationFix`). Z placement: half the brick height above the hit, minus
  one brick height when hitting a ceiling (normal z < −0.9), −0.1 on terrain,
  ±0.05 rounding bias (`%posZ -= %data.brickSizeZ`@s:15145). **X/Y are not
  snapped in script.** Grid snapping happens in the engine (04 §3). **[V]/[E]**
- Switching to a different brick keeps the ghost in place and applies a
  0.2-unit parity correction when odd/even footprint dimensions differ
  (`%shiftX = 0.2`@s:16464). Print bricks show your last print for that aspect
  ratio, else "A". The ghost takes your current paint color. **[V]**

### 4.2 Shift, rotate, plant, cancel, undo
- **Shift** sends `shiftBrick(x, y, z)` with x = away(+)/towards(−),
  y = left(+)/right(−), z in plates (±1 for 1/3, ±3 for full brick)
  (`shiftBrick`@c:4102). The server maps x/y **relative to the player's
  forward vector, snapped to the dominant world axis**, then moves 0.5 units
  per stud and 0.2 units per plate (`%x *= 0.5`@s:5445). **[V]**
- **Super-shift** moves by the brick's own footprint and height, respecting the
  rotated footprint (`ServerCmdSuperShiftBrick`@s:5452). **[V]**
- **Rotate** CW/CCW: quarter turns, with a ±0.25 re-centering when the
  footprint has odd×even dimensions (`ServerCmdRotateBrick`@s:5561). **[V]**
- **Player animations** on thread 3 for each action: shiftUp, shiftDown,
  shiftLeft, shiftRight, shiftAway, shiftTO, rotCW, rotCCW, plant, undo
  (`playThread(3, shiftUp)`@s:5379). Wrench, hammer and activate have their own
  (`activate2` after 5 rapid activates, `activateLevel`@s:9510). **[V]**
- **Key repeat** (client script): first action on key down, first repeat
  after **200 ms**, then every **50 ms** while held. It applies to shift, super
  shift and **plant** (`$BrickRepeatTime`@c:3973). Repeats are cancelled by
  release, via a per-key counter (`$brickAway++`@c:4098). **[V]**
- **Plant** (`ServerCmdPlantBrick`@s:5784): refused silently during mission
  cleanup, when building is disabled in the mini-game, or when over the
  per-second limit for non-admins (`MaxBricksPerSecond`@s:5813). Limit →
  `MsgPlantError_Limit`. Too far (distance from player >
  `$Pref::Server::TooFarDistance` (default 50, min 20) + brick radius) →
  `TooFar` (`TooFarDistance`@s:5849). Otherwise the engine plant check returns
  0 OK / 1 Overlap / 2 Float / 3 Stuck / 4 Unstable / 5 Buried / other
  Forbidden. **After a successful plant the ghost stays** (with your current
  color), ready for the next plant. **[V]**
- **Cancel** deletes the ghost (`ServerCmdCancelBrick`@s:5973).
- **Undo** (Ctrl+Z) pops a per-client stack of PLANT (removes the brick unless
  that would chain-kill others), COLOR, COLORFX, SHAPEFX, PRINT and
  COLORGENERIC (vehicles/bots) entries, each trust-checked
  (`ServerCmdUndoBrick`@s:5689). **[V]**
- **Plant error feedback**: the matching image plus the optional error sound
  (off in stock defaults, `PlantErrorSound`@d:52), hidden after 800 ms
  (`hideSchedule`@c:7201). **[V]**
- **Build macros** (record/playback of shift/plant sequences) exist
  (`ToggleBuildMacroRecording`@c:17521). They are nice-to-have and not required
  by the alpha **[M]**.

### 4.3 Mouse brick controls
`clientCmdUseBrickControls` would rebind the mouse to move and rotate the
ghost, but it `return`s immediately. **Dead code in v20. Do not implement**
(`clientCmdUseBrickControls`@c:10505). **[V]**

### 4.4 Super-shift toggle
With stock prefs (`UseSuperShiftToggle = 1`, `UseSuperShiftSmartToggle = 1`,
`UseSuperShiftToggle`@d:127), pressing Alt flips super-shift on. **Releasing
after more than 200 ms flips it back** (hold-to-use). A quick tap leaves it
toggled (`%minTime = 200`@c:5221). The badge shows while on. Alt+direction
works either way. **[V]**

## 5. Tools

| Tool | Reach | Rules | Evidence |
|---|---|---|---|
| Hammer | 5 units (5.5 looking steeply down) × player scale | Breaks a brick unless that would chain-kill (`willCauseChainKill`); trust ≥ Full (2), or you started the stack; damages players in mini-games; flips vehicles (impulse = mass × 5) | `%range = 5`@s:10496, `hammerImage::onHitObject`@s:10536 |
| Wrench | 10 units | Opens the wrench variant; trust ≥ Build (1) or admin override; miss/hit sounds | `wrenchImage::onFire`@s:10824 |
| Printer | 10 units | Only print bricks (`printAspectRatio` set); trust ≥ Full; opens Print Selector | `printGunImage::onFire`@s:23992 |
| Spray can | projectile | Paint projectile colors the brick (trust ≥ Full, pushes COLOR undo); colors vehicles/bots when allowed; FX cans set colorFX/shapeFX | `paintProjectile::OnCollision`@s:12347 |
| Empty hand click | 10-unit ray | `Player::ActivateStuff`: brick activation needs ≤ `$Game::BrickActivateRange` 5 × scale; vehicles and other objects any distance within the ray | `$Game::BrickActivateRange`@s:2862, `ActivateStuff`@s:9472 |

Trust constants: None 0, Build 1, Full 2, You 3. Paint, FX, Print, Hammer and
UndoBrick need 2. Wrench, BuildOn and RideVehicle need 1. The wrench's
render/collision/raycast/events options need 2 (`$TrustLevel::Hammer`@s:21244).
**On LAN every check passes** (`$Server::LAN`@s:21268). **[V]**

## 6. Chat and communication rules

- Channels: SAY (T) and TEAM (Y). The input is 120 characters max. The server
  also enforces `$Pref::Server::MaxChatLen`, flood and repeat protection
  ("Do not repeat yourself.") (`Do not repeat yourself`@ms:1111).
- `/command args` sends a server command, space-separated. Stock
  user-facing commands include `/sit`, `/hug`, `/alarm`, `/zombie`, `/light`
  and `/suicide`. Admin commands include `/clearbricks`, `/fetch`, `/find`,
  `/timescale` and more (see `serverCmd*` in `data/v20-function-index.tsv`). **[V]**
- The curse filter is client-side, with a stock word list; B4v21 turns it off.
- Chat lines stay 6.5 s. PageUp/PageDown scroll the history (1000 cached
  lines). "Press Up to Repeat Chat" is optional. **[V]**

## 7. Easily overlooked interaction rules (checklist seeds)

1. Pressing the active brick slot's number again **deselects** it (c:3706).
2. Pressing B while the selector is open = DONE (buys). Esc = cancel (c:4442).
3. Right-click in the selector = instant use without changing the cart (c:10324).
4. Clicking a cart slot, then another, **swaps** them. Clicking twice clears (c:10181).
5. Favorites store brick **names**. Missing names silently leave empty slots (c:10447).
6. The first spawn auto-buys favorites **slot 1** (c:7076).
7. Brick icons in the HUD are tinted with the paint color (c:6095).
8. E cycles paint **columns**, not colors. The wheel picks colors (c:4590).
9. The ghost stays after planting. Plant can be **held** to auto-repeat (c:4276).
10. Shift directions follow **body facing**, not camera. They change as you turn (s:5403).
11. Up/down steps are 3 plates. 1/3 steps are separate keys (c:4182, c:4217).
12. Zoom and free look are **hold** actions (c:20871, c:20885).
13. F5 hides the crosshair **and** player names together (c:5882).
14. Wrench "Copy" locks carry values to the next brick (c:15558).
15. The Events dialog always keeps a trailing empty row. Clearing a row's
    input deletes the row (c:18069).
16. Event delays are clamped to 0–30000 ms (c:17982).
17. Escape on the avatar screen **reverts**. Done applies and also closes Options (c:12280, c:13391).
18. Wrench dialogs suppress Left Shift so typing doesn't crouch (c:15829).
19. The mouse wheel does nothing to inventory while a menu or dialog is open (c:4463).
20. Chat hides when `LineTime` ≤ 0. T/Y are then ignored (c:5346).
21. Scrolling with the wheel while zoomed changes zoom, not inventory (c:4492).
22. Walking (C) applies immediately to already-held movement keys (c:20821).
23. Paint box slides in **instantly**. Brick and tool boxes animate (c:4952 vs c:4938).
24. On LoadingGui, Esc disconnects immediately in stock v20 (c:9241).

## 8. Network pref screen (adaptation note)

Connection types set packet size and rates: Dial-Up (240 bytes, 16/20 Hz)
and Broadband (1023, 32/32). Custom lets sliders edit them (`$Net_PacketSize`@c:5399).
These are Torque networking knobs. For the rewrite, replace them with an
informational panel, or remove the tab while keeping the tab strip order
**[M]**.
