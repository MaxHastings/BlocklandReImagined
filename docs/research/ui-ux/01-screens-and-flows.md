# 01 — Screens, layouts and flows

Evidence levels and citation keys are defined in [README.md](README.md). Exact
control trees (every control with source line, rect and fields) are in
`data/gui-layouts/<GuiName>.md`. Offscreen reconstructions are in
`renders/<GuiName>_1024x768.png`; `_annotated` variants outline named controls
with their source line.

All authored GUIs are 640x480 designs. The canvas stretches every top-level GUI
to the window size, and children then move or stretch by their `horizSizing` /
`vertSizing` (04 §1.1). Most dialogs are fixed-size windows centered on screen.
The main menu scales relatively, and the HUD is computed by script from the
screen resolution.

---

## 1. Startup and first run

Order, from `MainMenuGui::onWake`@c:13865:

1. `keyGui` if the build is not unlocked (authentication/key entry). **Out of scope**
   (no account services in the alpha).
2. `defaultControlsGui` when `$Pref::Input::SelectedDefaults == 0`, when
   `config/client/config.cs` is missing, or when the action map has fewer than 5
   binds (`defaultControlsGui`@c:13893). Also runs `vendorSpecificDefaults()`
   first. **[V]**
3. `SelectNetworkGui` ("Select Network Type": Dial-Up / Broadband) when
   `$Pref::Net::ConnectionType <= 0` (`SelectNetworkGui`@c:13897). **[V]**
   Recommended adaptation: skip it. It only sets packet rates (02 §8).
4. Every main-menu wake rebuilds the face and decal IFL lists from
   `Add-Ons/Face_*/*.png` and `Add-Ons/Decal_*/*.png` (`buildIFLs`@c:14186).
   The avatar screen's face/decal menus come from these lists. **[V]**

`defaultControlsGui` ("Default Controls", 237x347 centered, `g:19965`):
"Select your hardware configuration:". **Mouse:** One Button / Two Button /
Two Button + Wheel (a hidden fourth option is "Tilt Wheel"). **Keyboard:**
Standard Keyboard / Laptop Keyboard (No NumPad). Buttons are `<< Cancel` and
`Apply >>`. Preselected: *Two Button + Wheel* and *Standard Keyboard*
(`OPT_Mouse2`@c:15233). Cancel is covered by a grey blocker until defaults have
been chosen once, so the first choice is mandatory
(`DefaultControls_CancelBlocker`@c:15236). Choosing re-creates the whole
action map (`new ActionMap(moveMap)`@c:15311). The effect on binds is in 02 §1.
The dialog also opens 1 s after spawning if the map has fewer than 5 binds
(`defaultControlsGui`@c:7081). **[V]**

## 2. Main menu (`MainMenuGui`, g:4514)

Render: `renders/MainMenuGui_1024x768.png`.

- Content GUI (`GuiChunkedBitmapCtrl`). Background `MM_BG` plus fader `mm_Fade`
  (`GuiFadeinBitmapCtrl`: `fadeinTime`@g:4555 2000 ms, wait 2000 ms, fadeout 100 ms).
  Pictures are random picks from `screenshots/*.png|jpg` (`buildScreenshotList`@c:14151).
  With no screenshots, the background is blank. The install has 10 JPGs in
  `screenshots/` **[V]**. Whether they shipped with retail v20 is **[M]**.
- Title bitmap `./title.png` (1065x195 source art drawn at 404x80) at 3,12.
  The version label shows "Version: 20" at runtime (`MM_Version`@c:13877).
- Big text-art buttons, all 224x40 with `relative` sizing, so they scale with
  the window. Top to bottom:
  Tutorial (y120) → `MM_Tutorial()`, Start a Game (y160) → `startMissionGui`,
  Join a Game (y200) → `JoinServerGui`, Player (y240) → `AvatarGui`,
  Options (y280) → `optionsDlg`, and Quit (y422). About and Credits sit at the
  bottom right (160x30). "© 2009 Blockland LLC" is at bottom-right.
- **Hover sounds** form an ascending synth scale (only when
  `$Pref::Audio::MenuSounds`): Tutorial Note3, Start Note4, Join Note5,
  Player Note6, Options Note7, Quit Note0, About Note1, Credits Note2
  (`MM_TutorialButton::onMouseEnter`@c:14226, `Note3Sound`@c:14230). Notes are
  `base/data/sound/notes/Synth 4/Synth4_NN.wav` (`Note0Sound`@c:78). **[V]**
- **No title music.** `TitleMusic` references `base/data/sound/music/Ambient Deep.ogg`
  (`TitleMusic`@c:137), which is absent from the install, and
  `MainMenuGui::PlayMusic` has no caller in any v20 script. **[V]** Silent menu,
  unless Maxwell remembers otherwise **[M]**.
- The auth bar (`MM_AuthBar`, "Demo Mode", Name/Key buttons) and the Buy Now /
  Demo banners are **out of scope**. Hide them or replace them with a neutral
  LAN name field.
- Tutorial loads `Add-Ons/Map_Tutorial/tutorial.mis` as single player
  (`MM_Tutorial`@c:14356). **That map is missing from Maxwell's install**
  (manifest). Decide: hide the button, or point it at Bedroom **[M]**.
- Quit asks "Quit to Desktop?" (`quitGame`@c:20690).

## 3. Start Game / map selection (`startMissionGui`, g:5037)

Render: `renders/startMissionGui_1024x768.png`. This is a full 640x480 window
titled "Start Game".

- Left: map list (`SM_missionList`, `GuiTextListCtrl` in a 243x260 scroll).
  Rows come from `$Server::MissionFileSpec`, filtered by `clientIsValidMap`
  and sorted by name (`SM_missionList.sort`@c:7465). Display name =
  `MissionInfo.name` read from the `.mis` text, falling back to the file base
  name (`getMissionDisplayName`@c:7554). **[V]**
- Right: preview `SM_MapPreview` (348x260). The image is `<mission>.png`, else
  `.jpg` beside the `.mis`, else `base/data/missions/default` ("UNKNOWN MAP")
  (`getMissionPreviewImage`@c:7507). The description, from `<mission>.txt`
  (else "..."), is drawn **over the bottom of the preview**, bottom-aligned
  (`SM_MapDescription.resize`@c:7657). **[V]**
- Server type radios: Single Player / LAN / Internet. Max Players popup 1–32.
  Server Name, Password, Admin Password and Super Admin Password fields. Also
  Advanced Config, Music Files and Add-Ons buttons. All of these are covered by
  a grey blocker in Single Player (`SM_OptionsBlocker`@c:7676). **[V]**
- `<< Back` (Esc) and `Launch Game >>`. Launch creates the server with the
  chosen type, shows `connectingGui` "Connecting to Local Host...", and
  connects locally (`SM_StartMission`@c:7525). Single player runs the load at
  time scale 10 and restores 1 after mission download (`setTimeScale`@c:7536,
  c:2151). **[V]**
- Alpha adaptation: keep Single Player and LAN. Replace Internet with "Direct
  IP host" semantics or hide it (no master server). Show Bedroom, Kitchen and
  Slopes from converted content, with their original preview images.

## 4. Join a Game (`JoinServerGui`, g:5517) and Connect to IP (`ManualJoin`, g:9993)

- JoinServerGui is a server browser (Pass, Ded, Server Name, Ping, Players,
  #Bricks, Map columns). Buttons: `<< Back`, `Query LAN`, `Query Internet`,
  `Connect to IP`, `Join Server`. Internet queries a master server
  (out of scope). **LAN query** broadcasts `queryLanServers` on ports 28050,
  28000 and 28051 (`queryLanServers`@c:8093). **[V]**
- ManualJoin, "Connect to IP" (224x157): Password field, Server IP field,
  `<< Cancel`, `Connect >>` → `MJ_connect()`, which shows connectingGui and
  connects with LAN name, net name and clan tags (`MJ_connect`@c:7836). **[V]**
- Alpha: LAN list + Connect to IP are the required paths; hide Internet.

## 5. Connecting and Loading

- `connectingGui` "Connection Attempt" (359x218): one text line and
  `<< Cancel`. Esc cancels.
- `LoadingGui` is content, not a dialog (`Canvas.setContent("LoadingGui")`@c:2162).
  Map picture full-screen (`LOAD_MapPicture`, stretched). Progress bar at
  22,420 596x25 with label text (`LoadingProgressTxt`). The text is
  "WAITING FOR SERVER" → "LOADING OBJECTS" → "LIGHTING MISSION (This only
  happens once)" by phase (c:2059, c:2085, `LIGHTING MISSION`@c:2108).
  The map name and description controls exist but are hidden. Chat is visible
  during loading (`LoadingGui::onWake`@c:2470). Esc disconnects (or opens the
  escape menu with the B4v21 patch). **[V]**
- On download complete the client rebuilds the brick selector, wrench menus
  and mini-game lists from the server's datablocks
  (`onMissionDownloadComplete`@c:2136). The UI therefore reflects **the
  server's** content set. **[V]**
- The "GHOSTING" animated indicator (`HUD_Ghosting`, 13 frames `Ghosting_00..12`)
  shows while the initial object ghost is loading
  (`clientCmdSetLoadingIndicator`@c:6910).

## 6. In-game HUD (`PlayGui`, g:1521 + runtime construction)

Renders: `renders/Dyn_HUD_{Idle,Bricks,Paint,Tools}_1024x768.png` (runtime
formulas reconstructed by `tools/dynamic_scenes.py`).

`PlayGui::onWake` pushes the chat HUD and the move map
(`Canvas.pushDialog(NewChatHud)`@c:5972). The authored `HudInvBox` in the GUI
file is **legacy**: the live HUD is created in script whenever it is rebuilt
(`resetCanvas`, bricks loaded, tool HUD message). **[V]**

### 6.1 Brick bar (bottom center) — `PlayGui::createInvHud`@c:6027
- 10 slots (`$BSD_NumInventorySlots`@c:10125). Icon size is
  `min(64, floor(screenW/10))`. The box is centered and flush with the bottom
  edge, translucent black (`0 0 0 0.25`). Each slot has an inner darker swatch
  inset 2/4 px (`%newSwatch.setColor`@c:6069).
- Slot numbers "1…9,0" drawn at each slot's top-left when
  `$pref::Gui::ShowBrickSlotNumbers` (`ShowBrickSlotNumbers`@c:6103).
- **Icons are tinted with the current paint color**, alpha clamped to ≥0.1,
  when `$pref::Hud::RecolorBrickIcons` (default on)
  (`RecolorBrickIcons`@c:6095). Before any paint is chosen the index is color
  0, so the icons start red. Missing icons use `brickIcons/unknown.png`.
- Selection overlay `brickIcons/brickIconActive` over the active slot
  (`setActiveInv`@c:3664).
- Name bar, 18 px, directly above: blue corner caps (`BlueHudLeftCorner`/`Right`)
  with a translucent navy middle (`0 0 0.5 0.5`). It holds three overlaid
  texts: centered brick name, a left tooltip "  Press 1 or  2 3 4 5 6 7 8 9 0
  to use bricks", and a right tooltip "Press B for more bricks   ". The key
  names are read from current binds (`ToolTip_Bricks`@c:6185). With stock
  binds `useFirstSlot` is unbound, which gives the double space. **[V]**
- Hidden when not in brick mode (`$pref::HUD::HideBrickBox`, default on): it
  slides down 64 px (87 px without tooltips), so only the name bar remains at
  the screen bottom. The slide is 10 steps × 10 ms
  (`hideBrickBox`@c:6214, `hideBrickBox(-64`@c:4938). **[V]**

### 6.2 Paint box (bottom left) — `PlayGui::LoadPaint`@c:6240
- One **column per color division** of the server colorset plus a final FX
  column. Swatches are 16x16 on a 17 px pitch. The default colorset has 4
  divisions of 9 (Standard, Bold, Soft, Transparent) (`DIV:Standard`@s:12482).
  Column height = largest division.
- FX column: dark grey "none", then Pearl, Chrome, Glow, Blink, Swirl, Rainbow,
  Stable and Undulo bitmaps (`FXpearl.png`@c:6402). The label for the 9th is
  **"FX - Undulo"**, backed by `FXjello.png` (`FX - Undulo`@c:4659).
- Non-selected columns are faded to alpha × 0.3 (`FadePaintRow`@c:6642).
  Selection frame `paintActive` 18x18.
- Right of the columns sits the **spray-can label**: `paintLabelBG`,
  `paintLabelBGLoop` (tiled), `paintLabel` tinted with the current color, and
  `paintLabelTop`, each 100x100 (`HUD_PaintIcon`@c:6324).
- Name bar above the columns shows "<Division> - <n>" or "FX - <name>"
  (`HUD_PaintName.setText`@c:4674).
- Tooltip "E = Paint" on `ItemIcons/toolLabelBG` next to the can
  (`ToolTip_Paint`@c:6550).
- Hidden when not painting: it slides left until only the can label and
  tooltip peek out (`hidePaintBox`@c:6580). It slides **in instantly**
  (1 step, `hidePaintBox((-1`@c:4952) and out over 10 steps. **[V]**

### 6.3 Tool box (top right) — `PlayGui::createToolHud`@c:6704
- `maxTools` slots (5 for the standard player, `maxTools`@s:8827) in a vertical
  column at the right edge on a tiled `itemIcons/ToolBG`. Default tools are
  Hammer, Wrench and Printer in slots 0–2 (`hammerItem`@s:9697).
- Missing tool icons fall back to the first letter of the item name from
  `Print_Letters_Default` (`%letterFile`@c:6751). Icons may be color-shifted
  per item (`doColorShift`@c:6766).
- Name label below the column on `toolLabelBG` shows the item name, or the
  tooltip "Q = tools" when idle (`ToolTip_Tools`@c:6810).
- Hidden: slides up by `slots × 64` (+25 without tooltips), leaving the label
  at the top-right corner. Hides over 20 steps and shows over 10
  (`hideToolBox($HUD_NumToolSlots`@c:4911). **[V]**

### 6.4 Other HUD elements
| Element | Behaviour | Evidence |
|---|---|---|
| Crosshair | `crossHair.png` 32x32 centered (GuiCrossHairHud). Toggled together with player names by F5 | `Crosshair`@g:1587, `ToggleShapeNameHud`@c:5882 |
| Player names | `GuiShapeNameHud` over the full screen (engine-drawn names) | `PlayGui_ShapeNameHud`@g:1607 |
| Plant errors | 255x75 image at 205,345 (relative sizing), or 90x70 top when `$pref::Video::useSmallPlantErrors`. Shown 800 ms; the error sound is optional | `handlePlantError`@c:7135 |
| Center print | ML text centered in a 619x203 box, 3 sizes (20/36/56 px). Auto-clears after N s | `clientCmdCenterPrint`@c:6921 |
| Bottom print | ML text at the bottom with an optional black 50% bar | `clientCmdBottomPrint`@c:6942 |
| Respawn countdown | Center print "Respawning in N seconds...", then "Click to respawn." | `respawnCountDownTick`@c:7020 |
| Energy bar | Hidden unless the server enables it (`showEnergyBar = 0` for the standard player) | `clientCmdShowEnergyBar`@c:6900, `showEnergyBar`@s:8830 |
| Super Shift badge | `supershift` bitmap bottom-right while super-shift is on | `HUD_SuperShift`@c:5228 |
| Lag icon | 32x32 top-right when the connection lags | `LagIcon`@g:1534 |
| Mouse tip | "TIP: Press M to toggle mouse and click on links" under chat when a link is shown | `MouseToolTip.setValue`@c:14952 |
| Demo brick count | Out of scope | `Demo_BrickCountBox`@g:2184 |

### 6.5 Chat HUD (`NewChatHud`, `newMessageHud`)
- Lines render top-left at 2,20 with `BlockChatTextSize<N>Profile`: Palatino
  Linotype, size 16+2N, default N=4 → 24 px, black outline
  (`OPT_SetChatSize`@c:5531). Up to `$Pref::Chat::MaxDisplayLines` (8) visible.
  Each line fades after `$Pref::Chat::LineTime` 6500 ms (`LineTime`@c:14900).
  A "VVV" indicator shows when scrolled up. **[V]**
- Message format (server): `\c7<clan prefix>\c3<name>\c7<clan suffix>\c6: <text>`
  (`chatMessageAll`@ms:1176). Palette: \c0 red-pink, \c1 blue, \c2 green,
  \c3 yellow, \c4 cyan, \c5 magenta, \c6 white, \c7 grey (`fontColors[3]`
  in `data/gui-profiles.md`). URLs become `<a:…>` links. The speaker's avatar
  plays the talk animation for `strlen × 50 ms` (`playThread`@ms:1120). **[V]**
- Input: T (SAY) or Y (TEAM) opens `newMessageHud` directly under the chat
  lines. It shows a channel label "SAY:" (\c0) / "TEAM:" (\c1) and a 120-char
  input (`NMH_Type`@g:19678). The typing indicator is sent on the first
  non-"/" character (`StartTalking`@c:14625). Empty text closes. `/cmd a b`
  sends `commandToServer(cmd, a, b…)` (`NMH_Type::send`@c:14630). Text that
  looks like an auth key is blocked. **[V]**

## 7. Escape menu (`escapeMenu`, g:12199)

Render: `renders/escapeMenu_1024x768.png`. A 221x421 centered window,
"Escape Menu", with 207x36 `button1` buttons. With
`$Pref::Gui::ColorEscapeMenu` (default on) each button is tinted
(`EM_PlayerList.mColor`@c:9256):

| Button | Tint | Action |
|---|---|---|
| Options | white | optionsDlg |
| Player List | green | NewPlayerListGui |
| Mini-Games | grey | create/join mini-game GUIs (out of alpha scope, keep disabled) |
| Admin Menu | yellow | adminGui, or AdminLoginGui if not admin |
| Save Bricks | blue | saveBricksGui (warning first when not local) |
| Load Bricks | cyan | loadBricksGui (admin or local only) |
| Disconnect | orange | "Exit to main menu?" (single player) / "Stop hosting server?" / "Disconnect from the server?" |
| Quit | red-orange | "Quit to Desktop?" |

Hover plays Note0…Note7 (`EM_Options::onMouseEnter`@c:9328). Esc toggles
(`escapeMenu::toggle`@c:9235). **On LoadingGui, Esc disconnects** in stock v20.
Disconnect prompts use `messageBoxYesNo` (`escapeFromGame`@c:20674). **[V]**

## 8. Options (`optionsDlg`, g:6267)

Renders: `renders/optionsDlg_Graphics_1024x768.png`, `optionsDlg_Controls_1024x768.png`.
A 630x470 window with `tab1` tab buttons: Graphics, Audio, Network, Controls,
Advanced, plus an "AvatarOptions" button that opens AvatarGui
(`optionsDlg::setPane`@c:2495). Done (Esc) closes.

- **Graphics**: Display Driver, Resolution, Bit Depth, Refresh Rate popups,
  Fullscreen, Disable Vsync, APPLY. Screenshot format. Gui Settings: Hide Paint
  Box, Hide Tool Box, Hide Brick Box, Small Plant Errors, and a checkbox
  labelled **"Show HUD" that actually drives `$pref::HUD::showToolTips`**
  (`Show HUD`@g:6847). Chat Size radio row 0–10 with an "Example Chat" preview.
  Quality radio columns (Minimum…Best): Brick FX, Texture, Particle, Shadow,
  Lighting and Physics ("Off"…"Best"). **[V]**
- **Audio**: driver list, Master/Shell/Sim volume sliders, and toggles for
  plant error sound, brick move sounds, brick plant sounds, music and menu
  sounds (`Menu Sounds`@g:8474).
- **Network**: connection type (Dial-Up / Broadband / Custom), sliders
  enabled only for Custom, and Download Sounds/Music/Textures and Disable UPnP
  (`SetConnectionType`@c:5423). Mostly obsolete. See 05 for the adaptation.
- **Controls**: remap list (Control Name | Key Binding, filled from
  `$RemapName`, 02 §2), Clear All, Remap All, Defaults >> (opens
  defaultControlsGui), mouse sensitivity slider (0.02–2.0, default 0.75),
  keyboard turn rate. Options checkboxes: Invert Mouse, Invert Mouse In
  Vehicles, Use Jump/Jet Combo, Reverse Brick Scrolling, Queue Brick Buying,
  Use Super Shift Toggle (reveals "Smart Toggle"), Fast 1st/3rd Person Switch
  (`Opt_SSSmartToggle`@g:8126). **[V]**
- **Advanced**: a scrollable pane of graphics toggles (clouds, sky, decals,
  precipitation, render player, trilinear, environment maps, textured fog,
  sharp filter, dynamic lights, terrain details/emboss, animated lights, jets in
  first person, VBO), draw distance and anisotropy sliders, particle falloff,
  grass. GUI options (colored escape menu, slot numbers, censor chat,
  re-color brick icons, chat line time, max chat lines, **temp-brick flash and
  inside/outside color settings**, press-Up-to-repeat-chat, show BL_IDs).
  Control options (strafe steering, steering auto-return). Misc (auto query
  master, auto light in dark maps) (`OptAdvGraphicsPane`@g:8746). **[V]**

## 9. Player Appearance / avatar customization (`AvatarGui`, g:13726)

Render: `renders/AvatarGui_1024x768.png`. A 547x480 window, "Player
Appearance". The preview is a 3D `GuiObjectView` (258x387) with camera rot
(0.3, 0.6, 2.52) and orbit distance 4.34 (`setOrbitDist`@c:12159).

- 3 columns × 5 rows of 64x64 part buttons (`btnDecalBG` background + preview
  icon + `btndecalA` button), each with a 32x32 color swatch button
  (`btnPartColor`) at its right:
  row 1 Face(head color) · Hat · Accent; row 2 Decal(torso) · Pack ·
  Second Pack; row 3 Chest · Right Arm · Left Arm; row 4 (Symmetry checkbox) ·
  Right Hand · Left Hand; row 5 Hip · Right Leg · Left Leg. **[V]**
- Clicking a part toggles a scrollable picker next to it (one at a time,
  `Avatar_TogglePartMenu`@c:12836). Part lists come from
  `base/data/shapes/player/{Hat,Pack,SecondPack,Chest,hip,RLeg,LLeg,RArm,LArm…}.txt`,
  `accent.txt`, and the face/decal IFLs (`AvatarGui_CreatePartMenu`@c:12173).
  Accent options depend on the chosen hat (`$accentsAllowed`@c:12182). **[V]**
- Clicking a color swatch opens a 6-wide grid of 32x32 colors from the avatar
  colorset (`$Avatar::Color`, default = the same 36 colors as the brick
  colorset, `ColorSetGui::defaults`@c:16700). Opaque-only unless the part
  allows transparency. The "Authentic" mode limits colors to basic ones
  (`$basicColors`@c:12744). **Symmetry** copies a limb color to its pair
  (`$pref::Player::Symmetry`@c:12626). **[V]**
- Bottom row: Color Set >> (avatar color editor `ColorSetGui`), Clan Tags
  prefix/suffix fields, LAN Name field, Randomize!, Done. Favorites 0–9 plus
  "Set Favs »" at the top, stored as `config/client/AvatarFavorites/<n>.cs`
  exports of `$Pref::Avatar::*` (`AvatarGui::ClickFav`@c:13414). **[V]**
- **Done** applies: sends `updatePrefs`, `updateBodyColors` and
  `updateBodyParts` to the server, and closes both AvatarGui and optionsDlg
  (`Avatar_Done`@c:13391, `updateBodyColors`@c:7790). **The window X /
  Escape reverts** all avatar prefs to their values at open
  (`AvatarGui::ClickX`@c:12280). **[V]**

## 10. Brick Selector and favorites (`BrickSelectorDlg`, g:12559)

Renders: `renders/Dyn_BSD_Tab0_1024x768.png` (Bricks tab) and `Dyn_BSD_Tab1` (Plates).

- Fixed 640x480 window, "Brick Selector" (B key; `openBSD`@c:4430). Opening
  hides the HUD boxes (`HUD_BrickBox.setVisible(0)`@c:9711). Pressing B again
  while open acts as DONE (`BSD_BuyBricks`@c:4444). Building-disabled servers
  show a center print instead. **[V]**
- **Tabs** (3,30; 80x25 each, `tab1` / active `tab1use`): one per brick
  `category`, in **datablock order**. Stock core: Bricks, Plates, Rounds,
  Special, Ramps, Baseplates (`data/v20-core-brick-datablock-order.tsv`).
  A hidden "next tab" button binds **Tab** to cycle tabs
  (`BSD_NextTab`@c:10353). **[V]**
- **Content**: per tab, a vertically scrolling list (3,57; 634x363) of
  `subCategory` sections. Each has an Impact heading ("1x", "2x",
  "3x Height"…), then a grid of 96x96 icon tiles, 6 per row on a 97 px pitch,
  starting at x=18. The brick name is centered in Arial 12 black along the
  tile bottom (`BSD_CreateBrickButton`@c:10051). Tile =
  `brickiconbg` + icon + hidden `brickIconActive` + `brickIconBtn` button.
  Section height uses 96 px per row while tiles use 97, so tall sections
  overlap the next heading by 1 px per row (`%numRows * 96`@c:10046). **[V]**
- **Cart** (bottom, blue `0.2 0.5 1 1` bar): 10 slots of 55x55 at 3,421
  (`BSD_CreateInventoryButtons`@c:10123). `Clear Cart` sits above the bar's
  left end, and DONE (68x42, Enter) is at the right.
- **Interaction rules** (`BSD_ClickIcon`@c:10254, `BSD_ClickInv`@c:10181):
  - Click a brick tile to select it (active overlay). Click the **same** tile
    again to add it to the first empty cart slot. If the cart is full and
    **Queue Brick Buying** is on (default), everything shifts left and the new
    brick goes last. **[V]**
  - With a tile selected, click a cart slot to place it there (replacing).
  - Click a cart slot to select it. Click another slot to **swap** them. Click
    the same slot again to **clear** it. **[V]**
  - **Right-click a tile** = "instant use": closes the selector and gives you
    that brick immediately, without touching the cart (`BSD_RightClickIcon`@c:10324).
  - DONE / Enter sends `BuyBrick(slot, data)` for all 10 slots, clears the HUD
    icons, and closes (`BSD_BuyBricks`@c:10363). **[V]**
- **Favorites**: buttons 1…9,0 in the title bar (19x19, keyboard accelerators
  1–9 and 0). Clicking loads that favorite set into the cart
  (`BSD_BuyFavorites`@c:10435) (DONE still required). "Set Favs>" toggles a
  helper "^^^ Click a number to set favorites ^^^" and the button text becomes
  " Cancel ". The next number click saves the current cart, **by brick uiName**,
  to `config/client/Favorites.cs`. An empty cart clears that favorite
  (`BSD_SaveFavorites`@c:10402). Empty favorites draw their button at 50%
  alpha (`updateFavButtons`@c:10489). **[V]**
- **First spawn auto-loadout**: on the first spawn of a session the client
  buys favorites slot 1 (`BSD_ClickFav(1)`@c:7076). Stock slot 1 = 32x32 Base,
  2x4, 2x2, 1x4, 1x2, 1x1, 1x16, 6x12F, 1x4x5 Window, Vehicle Spawn
  (`$Favorite::Brick1_0`@d:252). Slot 2 is a road/landscape set (d:262–271). **[V]**
- Esc closes without buying (`accelerator`@g:12580).
- Known authored quirk: both DONE and Clear Cart declare accelerator `return`
  (`BSD_ClearBtn`@g:12909). Enter behaves as DONE in practice **[A]** (engine
  accelerator precedence, 04 §1.4).

## 11. Print Selector (`PrintSelectorDlg`, g:12468)

214x439 window, "Print Selector", right of center, with **Prints** and
**Letters** tabs (`PSD_LettersTab`@c:9553). A 3-column grid of 64x64 print
icons (65 px pitch) in a 205x392 scroll (`PSD_LoadPrints`@c:9567). Icons are
`<print pack>/icons/<name>.png`. For letters, **keyboard accelerators type the
letter**: single-character names map directly, and symbols map to shift-keys
(`-bang` → shift 1, `-qmark` → shift /, `-space` → space…) (`shift 1`@c:9624).
Clicking applies the print through `setPrint` and remembers the last print per
aspect ratio. A newly deployed print brick shows the last print, or "A"
(`lastPrint`@s:6285, `letters/A`@s:15050). Opened by hitting a print brick
with the Printer (`openPrintSelectorDlg`@s:24023). **[V]**

## 12. Wrench dialogs (g:20206, g:20906, g:21098)

Renders: `renders/wrenchDlg_1024x768.png`, `wrenchSoundDlg…`, `wrenchVehicleSpawnDlg…`.

- The server opens the right variant by `specialBrickType`: Sound, then
  VehicleSpawn, else normal (`openWrenchSoundDlg`@s:10929). The title is
  "Wrench - <owner heading>". On LAN the heading is the owner's player name;
  online it is "<group name> - (BL_ID: n)" (`%netHeading`@s:10924). **[V]**
- A small "Loading..." window shows until data arrives
  (`Wrench_LoadingWindow`@c:15832).
- **Wrench** (297x373). Rows, each with a "Copy" lock checkbox on the right:
  Brick Name (text, max 32) · Light (popup) · Emitter (popup) · Emitter Dir
  (radios U D N E S W) · Item (popup) · Item Pos (U D N E S W) · Item Dir
  (N E S W) · Item Respawn Time (ms text) · Ray Casting · Collision ·
  Rendering. Buttons: `<< Cancel` (Esc), `Events`, `Send >>`. **[V]**
- **Copy locks** mean "keep my value": when the dialog is filled from a brick,
  locked fields are **not** overwritten (`WrenchLock_Name.getValue`@c:15558).
  Send still sends the locked values, which copies settings from brick to
  brick. The locks persist while the client runs. **[V]**
- Popups list datablocks by `uiName`, with " NONE" first: FxLightData,
  ParticleEmitterData, ItemData, looping AudioProfile (sound variant), and
  rideable vehicles/PlayerData (vehicle variant). Emitters, items, sounds and
  vehicles are sorted. **Lights are not sorted** (`Wrench_Emitters.sort`@c:15814).
  **[V]**
- Send packs `N/LDB/EDB/EDIR/IDB/IPOS/IDIR/IRT/RC/C/R` fields into one
  `SetWrenchData` string (`wrenchDlg::send`@c:15840). Admin override shows a
  grey blocker over Send. `$Pref::Server::WrenchEventsAdminOnly` puts a blocker
  over Events for non-admins (`Wrench_EventsBlocker`@c:15544). **[V]**
- **Sound** variant (323x179): Brick Name, Sound, Events. **Vehicle Spawn**
  variant (323x295): Brick Name, Vehicle, Re-Color Vehicle, Ray
  Casting/Collision/Rendering, `< Respawn >`, Events.
- While any wrench dialog is open, **Left Shift is unbound** so typing
  capitals does not crouch (`NoShiftMoveMap`@c:15829). **[V]**

## 13. Wrench Events (`wrenchEventsDlg`, g:24606)

Render: `renders/Dyn_WrenchEvents_1024x768.png`. An 800x600 window, "Wrench
Events", shrunk to the screen if smaller (`wrenchEventsDlg::onWake`@c:17787).
Column headers: Enable | Delay | Input Event | Target | Output Event | Output
Parameters. Clear button and a "Copy" lock at the top right. `<< Cancel` and
`Send >>` at the bottom.

- The first open requests event tables from the server
  ("Getting event types from server..."), then the brick's events
  (`RequestEventTables`@c:17852). **[V]**
- Each row is a 36 px tall translucent box
  (`wrenchEventsDlg::newEvent`@c:17953):
  `[✓ n]` enable checkbox whose label is the row index · Delay (36 px text,
  clamped 0–30000 ms on accept) · Input popup (100 px, sorted, "-" = none) ·
  Target popup, whose list depends on the input · optional named-brick popup
  **below** the target (`CreateNamedBrickList`@c:18105) · Output popup
  (sorted, filtered by the target's class) · parameter widgets.
- **Cascade rules**: choosing an input builds the target list and appends a
  fresh empty row, so there is always one trailing blank row
  (`%inputMenu.createdNew`@c:18075). Choosing "-" on a non-last row deletes
  that row and renumbers the rest (`reshuffleScrollbox`@c:18009). Changing a
  target to a different class rebuilds the output list and parameters.
  `<NAMED BRICK>` appears only if you own the brick
  (`allowNamedTargets`@s:10914). Named-brick lists are your group's names,
  sorted. **[V]**
- Parameter widgets by type (`createOutputParameters`@c:18248):
  `int` = text sized by max digits (validated and clamped); `intList` = text;
  `float` = 100x36 slider snapped to step; `bool` = checkbox; `string` = text
  with max length; `datablock` = popup with "NONE" first (special Music /
  Sound / Vehicle filters); `vector` = three 31 px fields; `list` = popup;
  `paintColor` = 18x18 swatch that opens a 6-wide palette grid. The first
  widget takes keyboard focus. Max 4 parameters. **[V]**
- Send = `clearEvents`, then one `addEvent(enabled, inputIdx, delay, targetIdx,
  NTNameIdx, outputIdx, p1..p4)` per row, then close events **and** the
  parent wrench dialog (`wrenchEventsDlg::send`@c:18514). Indices are the
  server's registration order, not the sorted display order. **[V]**
- If a named target is removed while the dialog is open, the dialog closes
  with "Named Target List Invalidated" (`ClientCmdRemoveNTName`@c:18690). **[V]**
- Stock tables: inputs `registerInputEvent`@s:17134–17142 (onActivate,
  onPlayerTouch, onBotTouch, onProjectileHit, onBlownUp, onRespawn, onRelay,
  onPrintCountOverFlow, onPrintCountUnderFlow). Outputs
  `registerOutputEvent`@s:17380–18391. The subset mapping is in 05 §P0-7.

## 14. Player list (`NewPlayerListGui`, g:2249)

F2 toggles (`showPlayerList`@c:4422). A 489x332 window titled
"<n> / <max> Players - <server name>" (`NPL_Window.setText`@c:485). Sortable
columns Admin | Name | Score | BL_ID (hidden unless the pref is set) | Trust.
Right-side action groups: Trust Invite (BUILD, FULL), Trust Demote
(NO TRUST, BUILD), Mini-Game (Invite, Remove), UN-IGNORE, Close. Buttons
disable (grey blockers) according to the selected player's state. **[V]**
Alpha: trust semantics conflict with stock LAN behaviour (README finding 1),
so decide what this panel does on LAN **[M]**.

## 15. Save / Load Bricks (g:12932, g:13260)

- **Save Bricks** (640x480): Name field, map preview, Description (ML edit)
  with Clear, "Save Wrench Events" and "Save Ownership" checkboxes, an
  existing-files list sortable by File Name/Date, Save (Enter) and Cancel.
  Files go to `saves/<MapSaveName>/<name>.bls` (`SaveBricks_Save`@c:11596),
  with overwrite confirmation. **The client writes the save file** from its
  ghosted bricks, first downloading names/events/owners from the server when
  "Save Wrench Events" is on (`SaveBricks_StartInfoDownload`@c:12083). Invalid
  filename characters are rejected. **[V]**
- **Load Bricks** (640x480): Map Name popup (save folders), file list,
  preview, description, "Load Brick Ownership", Load/Cancel. Local servers load
  directly (after a colorset match dialog `LoadBricksColorGui`); remote
  servers get the file **uploaded line by line** (`InitUploadHandshake`@c:11452).
  Load requires admin or a local server (`escapeMenu::clickLoadBricks`@c:9290). **[V]**
- Alpha decision **[M]**: v20's client-side save and upload model versus a
  server-authoritative save. The contract asks for authoritative multiplayer,
  so keep the dialogs but save and load on the host, and say so in the playtest
  guide.

## 16. Out-of-scope screens (present in v20; not required by the alpha)

Mini-games (`CreateMiniGameGui`, `joinMiniGameGui`, invites), admin tools
(`adminGui`, ban/unban, BrickManGui), server config/Add-Ons/Music Files lists,
auth/key/registration (`keyGui`, `regNameGui`), AutoUpdateGui, demo/recordings,
editors (Inspect, console). Recommendation: omit them from the alpha, but keep
the **Escape menu layout intact**, with omitted entries disabled rather than
removed, to preserve muscle memory **[M]**.
