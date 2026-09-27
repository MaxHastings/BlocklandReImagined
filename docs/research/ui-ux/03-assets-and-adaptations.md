# 03 — UI assets, skins, fonts, sounds and required adaptations

Evidence levels and citation keys are defined in [README.md](README.md). The
full per-reference manifest, with resolved path, size and SHA-256, is
`data/ui-asset-manifest.md` / `.json`. All assets are read from Maxwell's
install. Nothing is copied into the repository.

## 1. Inventory summary

`tools/asset_manifest.py` scanned every literal asset path in the vanilla GUI
and client script decompiles. It resolves loose files and Add-On ZIP members,
expands bitmap-button `_n/_h/_d/_i` states and animated `_00…` frames.

| Status | Count | Meaning |
|---|---:|---|
| ok | 241 | resolved to a file |
| ok-states | 20 | bitmap button base whose state images all exist |
| ok-frames | 1 | `Ghosting_00…12` animated loader |
| runtime-file | 3 | `config/client/Favorites.cs`, `avatarColors.cs`, `prefs-trustList.txt` (created by the game) |
| compiled-script / directory | 2 | not assets |
| missing-editor-help | 8 | Torque editor help pages (irrelevant) |
| **missing** | **18** | see §5 |

Many files are referenced **dynamically** (paths built at runtime), so they are
not literal references. Examples: 136 brick icons via datablock `iconName`,
`plantErrors*/PlantError_<Kind>` concatenation (c:7137–7189), tool icons via
item datablocks, print icons via `getPrintTexture`, and avatar part icons from
`base/data/shapes/player/*.txt`. The manifest lists 222 such unreferenced files
under `base/client/ui` for cross-checking.

## 2. Skins (GuiControlProfile bitmaps)

The whole look comes from a handful of profiles (`data/gui-profiles.md`;
`uses` counts are static controls only):

| Profile | Font | Skin bitmap | Used for |
|---|---|---|---|
| `GuiDefaultProfile` (569 uses) | Arial 14 | — | containers, swatches |
| `BlockButtonProfile` (265) | **Impact 18**, black, hover white | `blockScroll.png` array | every Blockland pill button, tabs, headings |
| `GuiCheckBoxProfile` (177) / `GuiRadioProfile` (73) | Arial 14 | `torqueCheck.png` / `torqueRadio.png` arrays | checkboxes/radios |
| `BlockWindowProfile` (51) / `GuiWindowProfile` (21) | **Impact 18** white | `blockWindow.png` array | all windows: blue gradient title bar, grey body `200 200 200` |
| `BlockTextEditProfile` / `GuiTextEditProfile` | Arial 14 | — (white fill, 1px border) | text fields |
| `ColorScrollProfile` / `BSDScrollProfile` / `BlockScrollProfile` | — | `halfScroll.png` / `blockScroll.png` arrays | scrollbars |
| `BlockChatTextProfile` + `Size0…10` | **Palatino Linotype** 16–36, outline | — | chat |
| `HUD*Profile` | Arial 12/14 white | — | HUD labels |
| `MM_LeftProfile` etc. | Arial 14 white, blue outline `24 24 255` | — | main-menu labels |

Bitmap arrays are single images whose pixel (0,0) is a **separator color**
(red). Pieces are cut out row by row (04 §1.2; `bitmap_array` in
`tools/gui_render.py`). `blockWindow.png` (61x158) holds close / maximize /
restore / minimize icons in 3 states, focused and unfocused title-bar
left/right/middle pieces, left and right edges, and bottom-left / bottom /
bottom-right pieces. **[V]** (pixels) **[E]** (piece indices, Torque3D
`guiWindowCtrl.h`, the `BorderTopLeftKey = 12` enum).

Bitmap buttons draw `<name>_n|_h|_d|_i.png` (normal, hover, down, inactive)
**stretched to the control rect**. The art is often much larger than the
control (for example, main menu `btnStartServer_n.png` is 499x72 drawn at
224x40), so the art is downsampled. The Blockland-specific `mColor` field
tints the button (escape menu colors). `lockAspectRatio`, `overflowImage` and
`alignLeft` are Blockland extensions with no reference source **[A]** (04 §1.3).

## 3. Fonts: recoverable from the install

The UI names Windows fonts (Impact, Arial, Arial Bold, Palatino Linotype,
Verdana, Book Antiqua, Lucida Console). **Every size the UI uses exists as a
pre-rendered Torque font cache** in `base/client/ui/cache/*.gft` (39 files).
These are original game data. The format is fully decoded in `tools/gft.py`:
16-byte header, 224 nine-byte glyph records, embedded PNG atlas sheets, and a
256-entry u16 codepage remap. Coverage is character codes 32–255
(Windows-1252/Latin-1). **[V]** (all 39 files parse; sample strips were
rendered and inspected.) The layout **differs from Torque3D's `GFont::read`**,
which expects a face-name string and ascent/descent.

Adaptation recommendation:
- Import the `.gft` atlases through the one-time conversion pipeline as
  native bitmap fonts, just like other original assets. This gives exact v20
  glyph shapes and metrics, avoids redistributing Microsoft font files, and
  works identically on Windows, macOS and Linux.
- Chat and names need characters beyond Latin-1 only if players type them.
  Add a fallback font for unsupported code points, and state this in the
  known-issues list **[M]**.
- Text is drawn with baseline and outline rules from the profile
  (`doFontOutline`, `fontOutlineColor`). The outline is a 1-pixel
  4-neighbour stroke in the reconstructions **[A]**.

## 4. UI sounds

| Event | Sound | Evidence |
|---|---|---|
| Main/escape menu hover | `notes/Synth 4/Synth4_00…08.wav` (ascending) | `Note0Sound`@c:78 |
| Plant error (optional, off by default) / generic error | `error.wav` | `AudioError`@c:24 |
| Item pickup | **also `error.wav`** (stock profile points at error.wav) | `ItemPickup`@c:30 |
| Admin / brick clear / player join / leave / upload start-end / process complete | `admin.wav`, `brickClear.wav`, `playerConnect.wav`, `playerLeave.wav`, `uploadStart/End.wav`, `processComplete.wav` | c:36–72 |
| Ghost brick move / plant / rotate / change | `clickMove.wav`, `clickPlant.wav`, `clickRotate.wav`, `clickChange.wav` (3D, close range) | `BrickMove`@c:158 |
| GUI button hover/click | **none**: every profile's `soundButtonOver` is empty, and `buttonOver.wav` does not exist | c:19616, manifest |
| Title music | **none**: the referenced file is absent and never played | `TitleMusic`@c:137 |

Brick move, plant and rotate sounds are **played by the engine**. No script
calls them, but the executable contains `BrickMove`, `BrickPlant`,
`BrickRotate` and the two `$Pref::Audio::PlayBrick…Sound` names. **[V]**
(strings) **[A]** (exact trigger points: every shift step? only on success?).
`clickSuperMove.wav` exists in `base/data/sound` with no script profile.
It is probably engine-played on super-shift **[A]**.

## 5. Missing or dangling references (what the rewrite must decide)

| Reference | Where | Effect in v20 | Recommendation |
|---|---|---|---|
| `base/data/sound/music/Ambient Deep.ogg` | c:139 | silent menu (never played anyway) | keep silent **[M]** |
| `base/data/sound/buttonOver.wav` | c:20 | no hover sound | none |
| `Add-Ons/Map_Tutorial/tutorial.mis` | c:14359 | Tutorial button would fail | hide or repoint **[M]** |
| `plantErrors/PlantError_Teams.png`, `…/PlantError_Forbidden.png`, `plantErrors_small/PlantError_Forbidden.png` | built at c:7171–7189 | blank error image for those codes | reuse the "overlap" art, or render a text fallback |
| `avatarIcons/{Accent,Hat,Pack,SecondPack}/none` | g:14549 etc. | blank "none" tile in part menus | draw an empty tile (faithful) |
| `GUIBrickSide*.png` | c:7302 (dead code after `return`) | none | ignore |
| `blockRadio.png`, `colorRadio.png` | c:20177, c:20376 | profile bitmaps missing; radios fall back | use `torqueRadio` look **[A]** |
| `Face_FMJ` thumbnails | g:15915 | add-on not installed | ignore |
| `base/data/sound/music/musicList.cs`, `base/server/crapOns_Cache.cs`, `brickTop.png`, `torqueToolWindow`, `clipboard.gui` | various | dev/editor leftovers | ignore |

## 6. Stock v20 vs Maxwell's B4v21-patched install

B4v21 overrides (loose `allClientGuis.gui` / `allClientScripts.cs` executed
after the vanilla DSOs):

| Area | B4v21 change |
|---|---|
| Admin GUI | Adds Fetch / Find buttons |
| Player list | Widened window |
| Escape menu | Adds a "Servers" button; window lengthened, Disconnect/Quit moved down |
| Brick Selector | Adds tab-shifting arrow buttons (`ArrowLeft/Right_*` art, unreferenced by stock) |
| Options | FOV slider, brick repeat-time settings, auto-enable-add-ons toggle, slider fix |
| Server filters | Custom master server |
| Scripts | Print-selector exploit fix, custom master/auth servers, version label, key repeat in text boxes, download info, **escape menu usable while loading**, a ghost-shift fix in free camera |
| Defaults | See `data/client-defaults-stock-vs-installed.tsv` (strafe steering off, vehicle invert off, censor off, 1280x720, PNG screenshots, …) |

Maxwell's muscle memory may include some of these (for example, opening the
escape menu while loading) **[M]**. The contract says "Blockland v20"; this
audit treats stock as the baseline and B4v21 items as optional quality-of-life.

## 7. Default-enabled add-ons that the UI depends on

`defaultAddOnList.cs` (stock) enables, among others (d-list at
`.research/bl-decompiled/v20/server/defaultAddOnList.cs`):

- **Bricks** (brick selector): Brick_Arch (12, "Rounds"), Brick_V15 (4),
  Brick_Checkpoint (1), Brick_Treasure_Chest (2), Brick_Halloween (5),
  Brick_Teledoor (1), and Brick_Large_Cubes (9, already converted). The current
  `content/stock-catalog-003` covers core 136 + Large Cubes only. **[V]**
- **Wrench and event menus**: Light_Basic (8 lights), Light_Animated (4),
  Particle_Basic (7), Particle_FX_Cans, Particle_Grass, Particle_Player,
  Particle_Tools. These are the **only** sources of the Light and Emitter
  popups, which the contract's "light/emitter" events need. **[V]**
- **Items/weapons** (Item popup, `setItem`, `spawnItem`, one weapon for the
  alpha): Weapon_Gun, Bow, Spear, Sword, Rocket_Launcher, Push_Broom,
  Horse_Ray, Guns_Akimbo, Item_Key, Item_Skis.
- **Vehicles** (vehicle spawn wrench): Vehicle_Jeep (required), plus Ball,
  Flying Wheeled Jeep, Horse, Magic Carpet, Tank, Rowboat, Pirate Cannon.
- **Sounds** (sound wrench, `playSound`): Sound_Synth4, Sound_Beeps, Sound_Phone.
- **Prints**: Print_1x2f_Default, 2x2f, 2x2r, Letters, Monitor3, BLPRemote.
- **Emotes**: Alarm, Confusion, Hate, Love.

Implication: the alpha's "stock" content set should be **defined as core +
default-enabled add-ons**, or the playtest guide must explain which menu
entries are missing **[M]**.

## 8. Adaptation list (what the rewrite has to build or convert)

Ordered by dependency. Priorities are repeated in 05.

1. **Font import**: `.gft` → native bitmap font resources, plus a fallback.
2. **Skin import**: bitmap arrays (`blockWindow`, `blockScroll`, `halfScroll`,
   `torqueCheck`, `torqueRadio`, `torqueMenu`) sliced by the separator rule
   into named pieces; button state quartets; HUD bitmaps.
3. **GUI layout data**: convert the 640x480 authored trees for in-scope
   screens (01) into native layout files. Keep rect, sizing flags, profile,
   text, command intent and accelerator. Drop editor/Internet/auth screens.
   The generated `v20-static-objects.json` is a starting point, but it
   contains decompiled content, so treat it like other converted originals
   (local, not committed).
4. **Runtime HUD builders**: port the script formulas (01 §6) instead of the
   authored PlayGui children.
5. **Data-driven menus**: brick selector (categories from content), wrench
   popups (lights/emitters/items/sounds/vehicles from content), event tables
   (server-registered, with parameter types), print selector (print packs),
   avatar part lists (`*.txt` + face/decal lists).
6. **Colorset**: default brick colorset (36 colors, 4 divisions; s:12473–12515),
   plus the avatar colorset (same values; c:16704–16739). The BLS importer
   already keeps save palettes.
7. **Audio cue table** for §4, including engine-triggered brick sounds.
8. **Prefs mapping**: v20 `$pref::` names → native settings (keep names in the
   converter's mapping file for traceability).
