# 04 — Engine-provided behaviour and open questions

Scripts describe *what* v20 asks the engine to do. Many visible behaviours are
implemented inside the closed `blocklandv20.exe` (a modified Torque Game
Engine), so scripts cannot fully specify them. For each such behaviour, this
file states the best available evidence and what still needs confirmation.
Engine references are Torque3D at the repository's pinned commit
`d0de864ea26293e5e905c6ec1768f985376af3de` (`T3D:<path>:<line>`, fetched
read-only for this audit, not stored in the repo). Torque3D descends from TGE
but is **not** the v20 engine. Blockland-specific GUI classes and fields have
no public source.

## 1. GUI system

### 1.1 Canvas sizing and resize rules
- Every top-level GUI, content or dialog, is resized to the full canvas
  ("all bottom level controls should be the same dimensions as the canvas",
  T3D:gui/core/guiCanvas.cpp:1535). **[E]**
- Children then apply `horizSizing` / `vertSizing` against the old/new parent
  extent: `right` (fixed), `left` (keeps right margin), `center`, `width`
  (stretch), `relative` (proportional position **and** size, rounded). The
  resize is skipped if the result would be below `minExtent`
  (T3D:gui/core/guiControl.cpp:1348–1391). **[E]**
- Consequence: most v20 dialogs stay at their authored pixel size and center;
  the main menu scales everything proportionally; PlayGui children use
  authored positions from a larger design resolution and are superseded by
  script-built HUD elements. `tools/gui_render.py` implements exactly these
  rules. **[E]**
- Children are clipped to their parent's rectangle. Blockland hides the
  "next tab" accelerator button this way (positioned at y = −23 inside
  BrickSelector, g:12859). **[E/V]**

### 1.2 Bitmap arrays
- A profile with `hasBitmapArray` slices its bitmap into pieces separated by
  the color of pixel (0,0). It scans rows at column 0, pieces left to right,
  and each piece's height runs down to the next separator
  (T3D:gui/core/guiTypes.cpp:558–631). The `blockWindow.png`, `blockScroll.png`,
  `halfScroll.png`, `torqueCheck.png` and `torqueRadio.png` skins all follow
  it (red separators observed). **[E/V]**
- Window pieces: 12 button images (close/max/restore/min × normal/hover/down),
  then TopLeft/TopRight/Top for focused and unfocused, Left, Right, BottomLeft,
  Bottom, BottomRight (T3D:gui/containers/guiWindowCtrl.h:51–84). Corners are
  drawn unscaled and edges stretched (guiWindowCtrl.cpp:1300–1341). **[E]**
  Whether v20 swaps title pieces on focus loss is **[A]**. In v20 almost every
  dialog is modal, so this rarely matters.
- Window titles: left-aligned with `textOffset` 5,2 in Impact 18, centered on
  macOS (profile expression `$platform $= "macos"`, `GuiWindowProfile`@c:19643).

### 1.3 Bitmap buttons
- `bitmap = base` loads `base_n` (normal), `base_h` (hover), `base_d` (down)
  and `base_i` (inactive); the `_n` suffix applies when no other state does
  (T3D:gui/buttons/guiBitmapButtonCtrl.cpp:238–330). The image is stretched to
  the control rect (`drawBitmapStretch`, :501). **[E]**
- Blockland extensions seen in every v20 bitmap button: `mColor` (tint, used
  for the colored escape menu), `lockAspectRatio`, `alignLeft`,
  `overflowImage`, `mKeepCached`. Their exact semantics are **[A]**. The
  reconstruction treats `mColor` as a multiply tint, which matches the known
  escape-menu look.
- Button **text** is drawn by the profile (`BlockButtonProfile`: Impact 18,
  black, white on hover, centered). The hover text color is **[A]** and should
  be confirmed in playtest.
- No button click or hover sounds come from profiles (01 §2). Main and escape
  menus play notes explicitly.

### 1.4 Dialog stack, focus, accelerators, cursor
- `Canvas.pushDialog` layers dialogs over the content. Mouse and keyboard go
  to the topmost dialog, and modal profiles block lower layers. The in-game
  move map is pushed by PlayGui/LoadingGui and popped when they sleep
  (`moveMap.push`@c:5973). **[V]/[E]**
- Accelerators (`accelerator = "escape"`, `"return"`, `"tab"`, digits) are
  collected from awake controls into a map. The **first matching entry** wins
  on key press and also fires on key-repeat (T3D:gui/core/guiCanvas.cpp:608–645). **[E]**
  So Esc closes the topmost dialog that declares it. Digits 1–0 hit the
  favorites buttons while the selector is open. The duplicate `return` on
  Brick Selector DONE and Clear Cart resolves to DONE, which comes first in
  child order (g:12633 before g:12909) **[E/A]**.
- Tab and Shift+Tab move focus between tabbable controls when nothing else
  handles them (guiCanvas.cpp:595–606). **[E]**
- Cursor: PlayGui has `noCursor`. Dialogs show the cursor. `ToggleCursor` (M)
  forces it on in play for clicking chat links. Direct input is re-enabled on
  PlayGui wake (`activateDirectInput`@c:5971). **[V]**

### 1.5 Text rendering
- GuiTextCtrl draws single-line text with profile justify. GuiMLTextCtrl
  handles tags: `<just:center>`, `<font:…>`, `<color:…>`, `\c0…\c9` palette
  switches (profile `fontColors[n]`), `<a:url>` links, `<spush>/<spop>`, and
  bitmaps. Chat and center/bottom prints depend on these **[E]**. `\c` codes
  index `fontColors[0..9]` of the control's profile (chat palette in 01 §6.5).
- Font caches are bitmap fonts at fixed pixel sizes (03 §3). Text does **not**
  scale with resolution. Only relative-sized bitmaps do. **[V]** (cache
  format) **[E]** (renderer).

### 1.6 HUD controls
| Control | Engine behaviour | Status |
|---|---|---|
| `GuiCrossHairHud` | In Torque3D it renders only for a first-person player control object (T3D:T3D/fps/guiCrossHairHud.cpp:116–122). In v20 it is toggled by F5 together with names | **[E]**. Hidden in third person and free camera (engine `isFirstPerson` check; scripts never re-show it) |
| `GuiShapeNameHud` | Draws names over players, fading with distance (`distanceFade`, `verticalOffset`, T3D:T3D/fps/guiShapeNameHud.cpp:110–284) | **[E]**. Its authored white child swatch (g:1624) is not visible in play **[A]** |
| `GuiHealthBarHud` | Energy/health bars (hidden in v20 unless the server enables them) | **[E]** |
| `GuiAnimatedBitmapCtrl` | Frame sequence `<name>_00…` (Ghosting). Frame rate unknown | **[A]** |
| `GuiFadeinBitmapCtrl` | fade-in 2000 ms / wait 2000 ms / fade-out 100 ms, with `onWait`/`onDone` callbacks the menu script uses for its slideshow | **[V]** fields, **[E]** behaviour |
| `GuiObjectView` | 3D avatar preview with orbit camera. v20 sets `setCameraRot(0.3, 0.6, 2.52)` and `setOrbitDist(4.34)` (c:12158–12159). Mouse-drag rotation is **[A]** | |

### 1.7 Action maps
- Bound functions receive `%val = 1` on key down and `0` on key up. Key
  repeat events are **not** delivered to bound game functions, which is why
  v20 implements brick key-repeat itself with `schedule`
  (`repeatBrickAway`@c:3974). **[E/V]**
- The `bind` flags seen in `ActionMap::copyBind` (`SD`, `SDI`: scale,
  dead-zone, inverted) apply to analog axes (`copyBind`@.research/v20-dso/client/actionMap.cs). **[V]**
- Modifiers are part of the key identity: "ctrl z", "alt numpad8",
  "shift-ctrl p". `NoShiftMoveMap` works by pushing a map that binds `lshift`
  to nothing, which shadows Crouch while a wrench dialog is open. **[V]/[E]**

### 1.8 Screens that depend on engine data queries
Resolution/driver/bit-depth menus, the audio driver list, the LAN query and
ping, datablock enumeration for menus (`getDataBlockGroupSize`), spray-can
division queries (`getSprayCanDivisionSlot`, `getColorIDTable`), and
`getPrintTexture`. The rewrite must provide native equivalents backed by its
content database. **[V]** (calls in scripts)

### 1.9 Limits of the reconstructions in `renders/`
They approximate scroll bars (a flat bar, no thumb), popup menus, sliders,
checkbox states, ML text layout (no wrapping), text outline, and hover/down
states. 3D views are placeholders, and the Bedroom render from
`artifacts/native-interiors-lit` stands in behind HUD scenes. Use them for
**geometry, skin and typography**. They are not pixel truth.

## 2. Player, camera and movement (Astra's systems, UI-facing facts only)

| Behaviour | Evidence | Status |
|---|---|---|
| 3rd person orbit distance 8, tilt 0.261, vertical offset 0.75 | `cameraMaxDist`@s:8744 | **[V]** values, **[E]** camera model |
| 1st↔3rd transition speed (`$cameraSpeed` 5 vs 1000) | c:20903–20907 | **[V]** variable, **[A]** units |
| Free look (Z held) decouples camera yaw from body | `$mvFreeLook`@c:20889 | **[E]** |
| Zoom FOV via `setFov` (default 90, zoom 45, clamp 5–120 in datablock) | `cameraMinFov`@s:8748 | **[V]** |
| Jets: trigger 4, emitters `playerJetEmitter` / ground emitter within 4 units | `jetGroundDistance`@s:8789 | **[V]** |
| Crouch changes the bounding box to 1.25×1.25×1.00 ×4 | `crouchBoundingBox`@s:8784 | **[V]** |
| First-person does not render the player model; own jets hidden in first person by default | `renderFirstPerson`@s:8740, `renderMyJets` pref | **[V]** |

## 3. Building (engine-native parts)

| Behaviour | What scripts show | What is engine-only | Status |
|---|---|---|---|
| Ghost brick grid snap | scripts set raw hit positions with ±0.05 Z bias (s:15069–15088) | snapping X/Y to half-stud / stud grid depending on footprint parity, Z to plate grid, inside `fxDTSBrick::setTransform` | Lattice **[V]**: all 276,612 bricks in the install's 35 BLS saves sit at x/y multiples of 0.25 and z multiples of 0.1 (checked 2026-09-26). Which footprint-parity offset applies where is **[A]** (inferred from the ±0.25 / ±0.2 parity corrections at s:5591–5605 and s:16462–16484) |
| Plant validation | result codes 0–5 map to messages (s:5871–5965) | overlap, float (unsupported), stuck (inside player/objects), unstable, buried tests, and "support" rules | **[A]**. Needs a written native rule-set, validated against v20 behaviour by Maxwell |
| Ghost appearance | prefs `tempBrickFlash*`, inside/outside colors (d:55–65) | flashing translucent ghost; outside uses the paint color by default | **[E/A]**. Maxwell should confirm the look |
| Brick sounds | profiles only (c:152–176) | when move/plant/rotate/change play | **[A]** |
| Chain-kill check | `willCauseChainKill` (s:10550, s:5715) | which bricks lose support when one is removed | **[A]** (collapse/fracture is excluded; only the "refuse to break" rule matters) |
| Instant-use data | `if (%InvSlot = -1)` assignment always sets `instantUseData` (s:16423); `OnCollision` uses `currInv > 0` (s:15022) | net effect: slot 0 deploys through `instantUseData` | **[V]** quirk. Reproduce the behaviour: "every selected brick can be deployed" |

## 4. Open questions for Maxwell ([M])

1. Stock v20 or B4v21 behaviour as the baseline (escape menu while loading,
   FOV slider, wider player list, custom defaults)?
2. On LAN, keep v20's "everyone trusted" or enforce ownership as the contract
   says? If enforced, what does the Player List trust panel do?
3. Main menu: silent (stock), or add music?
4. Tutorial button: hide, or repoint to a map?
5. Crosshair in third person: shown or hidden?
6. Which default-enabled add-ons count as "stock" for the brick selector,
   wrench menus and events?
7. Save/Load: keep v20's client-side save and upload model, or host-side
   saving behind the same dialogs?
8. Out-of-scope escape-menu items (Mini-Games, Admin): greyed out, hidden, or
   minimal?
9. Import Maxwell's own `config/client/config.cs` binds and favorites on first
   run (with consent)?
10. Chat font: exact Palatino caches, including the large default size 24 at
    modern resolutions?

## 5. Unverified items to close by reverse engineering (optional)

- Blockland-specific GUI fields (`mColor`, `lockAspectRatio`, `overflowImage`,
  `alignLeft`, `mKeepCached`) and classes (`GuiSwatchCtrl`,
  `GuiAnimatedBitmapCtrl`).
- Exact ghost snapping and plant-check math (the largest behavioural risk for
  "building feels right").
- The engine sound triggers for brick actions.
- Mouse wheel sign convention and scroll direction.
