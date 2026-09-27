# Blockland v20 UI/UX, controls and interaction fidelity audit

Author: Claude (independent audit, 2026-09-26), commissioned by GPT-6 Astra for
Maxwell. Scope: menus, HUD, brick selector, favorites, tools, wrench/events,
settings, avatar customization, controls/defaults and interaction rules.
Physics, simulation and networking implementation belong to Astra; this audit
only states the fidelity requirements those systems must satisfy for the UI.

Nothing outside `docs/research/ui-ux/` was modified. The original installation
was read, never written. No game was launched and no input was automated.

## Documents

| File | What it answers |
|---|---|
| [01-screens-and-flows.md](01-screens-and-flows.md) | Every in-scope screen: layout, transitions, script behaviour, exact text |
| [02-controls-and-interaction-rules.md](02-controls-and-interaction-rules.md) | Default binds, first-run scheme choice, remap list, build/tool/inventory rules, timings, units |
| [03-assets-and-adaptations.md](03-assets-and-adaptations.md) | UI asset inventory, skins, fonts, sounds, missing files, stock vs B4v21 differences, packaging |
| [04-engine-behavior-and-uncertainties.md](04-engine-behavior-and-uncertainties.md) | Behaviour supplied by the C++ engine that scripts cannot explain; open questions |
| [05-fidelity-checklist.md](05-fidelity-checklist.md) | Prioritized checklist for implementation and for Maxwell's playtest |

Generated evidence (regenerate, do not hand-edit):

| Path | Contents | Committed? |
|---|---|---|
| `data/gui-layouts/*.md` | Control tree of all 69 authored GUIs with source lines, rects at 640x480 and absolute positions | local only |
| `data/v20-static-objects.json` | Parsed static objects (1,612 GUI controls, 114 profiles, audio profiles) | local only |
| `data/gui-profiles.md` | Every GuiControlProfile: font, colors, skin bitmap, usage count | yes |
| `data/ui-asset-manifest.{md,json}` | Every asset path referenced by UI sources, resolved against the install with size and SHA-256 | yes |
| `data/default-binds-apply.tsv` | Every default bind with enclosing conditions and line | yes |
| `data/client-defaults-stock-vs-installed.tsv` | Effective `$pref` defaults: stock v20 vs Maxwell's B4v21-patched install | yes |
| `data/v20-core-brick-datablock-order.tsv` | 136 core bricks in datablock order (determines brick-selector tab/section order) | yes |
| `data/v20-function-index.tsv` | Every function in the decompiled client/server scripts with line span | yes |
| `renders/*.png` | Offscreen layout reconstructions at 1024x768 from original bitmaps and font caches | local only |

"Local only" items are listed in `.gitignore` here because they embed original
art/fonts or transcribe decompiled original content (see `AGENTS.md`).

## Evidence levels used throughout

- **[V] Verified**: stated in v20 source at the cited line, or observed in the install files.
- **[E] Engine reference**: supported by Torque engine source (Torque3D commit
  `d0de864ea26293e5e905c6ec1768f985376af3de`, the same pin used by the rest of this
  repo). Blockland v20 runs on an older, modified TGE, so treat these as strong
  hints, not proof.
- **[A] Assumption**: inference that still needs confirmation by reverse
  engineering or Maxwell's playtest.
- **[M] Maxwell**: a product decision or a memory only Maxwell can supply.

## Citation keys

| Key | File (repo-relative) |
|---|---|
| `c:N` | `.research/v20-dso/client/scripts/allClientScripts-Vanilla.cs` line N |
| `g:N` | `.research/v20-dso/client/ui/allClientGuis-Vanilla.gui` line N |
| `s:N` | `.research/v20-dso/server/scripts/allGameScripts-Vanilla.cs` line N |
| `ms:N` | `.research/v20-dso/server/mainServer.cs` line N |
| `d:N` | `.research/bl-decompiled/v20/client/defaults.cs` line N (stock client defaults) |
| `T3D:path:N` | Torque3D reference source, `Engine/source/path` line N |

`` `name`@c:N `` is a checkable claim: identifier `name` appears within three lines
of the cited line. `tools/verify_citations.py` checks every citation and claim
in these documents (see "Verification" below).

### Source provenance

- `.research/v20-dso/` was decompiled by the previous agent from the DSO files
  in Maxwell's install (`*-Vanilla.*.dso`). After CRLF normalisation, the GUI
  decompile is byte-identical to the public `bl-decompiled/v20` copy. The script
  decompiles differ only in decompiler control-flow style (`continue` vs
  `else`), so logic is equivalent but line numbers differ by a few lines.
  **Always cite the `v20-dso` copy.**
- The install is **B4v21-patched**: loose `base/client/ui/allClientGuis.gui`
  and `base/client/scripts/allClientScripts.cs` exec the vanilla DSOs and then
  replace six GUIs and several functions. Its `base/client/defaults.cs` also
  differs from stock (for example, strafe steering off, vehicle mouse invert
  off, chat censor off, 1280x720). This audit describes **stock v20** and lists
  B4v21 deltas separately (03 §6). Which one Maxwell remembers is a **[M]**
  decision.

## Headline findings

1. **Stock LAN games had no ownership protection.** `getTrustLevel` returns
   `$TrustLevel::You` for everyone when `$Server::LAN` (`$Server::LAN`@s:21268).
   The alpha contract's LAN ownership enforcement is therefore a deliberate
   modernization. Implement it, but label it as such and decide what the trust
   UI shows on LAN **[M]**.
2. **"Stock brick catalog" is larger than the converted catalog.** Default-on
   add-ons (`defaultAddOnList.cs`) add about 25 more selector entries: Arch 12,
   V15 4, Halloween 5, Treasure Chest 2, Checkpoint 1 and Teledoor 1.
   `content/stock-catalog-003` has core 136 plus Large Cubes 9. The wrench
   Light/Emitter/Item/Sound menus and several event parameters are populated
   **only** by default-on add-ons (Light_Basic, Light_Animated, Particle_*,
   Weapon_*, Sound_*).
3. **Brick shifting is relative to the player's body facing**, snapped to the
   dominant world axis (server `ServerCmdShiftBrick`@s:5366). It is not relative
   to the camera. Units: 1 stud = 0.5 units horizontally, 1 plate = 0.2 units
   vertically, and up/down steps 3 plates.
4. **Key repeat for building is scripted**: 200 ms first delay, then every
   50 ms, while the key stays held (`$BrickFirstRepeatTime`@c:3972). Plant also
   repeats.
5. **Zoom (F) and Free Look (Z) are hold, not toggle.** The mouse wheel changes
   zoom FOV 5–85 in steps of 5 while zoomed (`toggleZoom`@c:20871,
   `$ZoomOn`@c:4492).
6. **The HUD is built at runtime**, not from the saved GUI. Brick bar, paint
   box and tool box geometry, tooltips and slide-in/out animations are all
   script formulas (`PlayGui::createInvHud`@c:6027). Brick icons are tinted
   with the current paint color, so they start red.
7. **UI fonts are recoverable from the install.** Every font size the UI uses
   exists as a pre-rendered Torque font cache (`base/client/ui/cache/*.gft`,
   39 files, Latin-1). The format is decoded in `tools/gft.py`. The UI can use
   the original glyphs without shipping Microsoft fonts; windows and buttons use
   Impact 18, chat uses Palatino Linotype.
8. **Stock v20 had a silent main menu.** `TitleMusic` points to a file absent
   from the install, and nothing calls `MainMenuGui::PlayMusic`. Menu hover
   sounds come only from the ascending synth notes on the main and escape menu
   buttons.
9. **The first run forces a control-scheme dialog.** Mouse type and
   numpad/laptop keyboard choose between two build keymaps and whether the
   wheel scrolls inventory (`defaultControlsGui::apply`@c:15272).
10. **The events dialog is data-driven**, from server-registered tables
    (`registerInputEvent`@s:17134). Stock v20 has 9 inputs and 65 outputs
    across 5 target classes, with up to 4 typed parameters per output. The
    alpha's event subset needs an explicit UI policy for unsupported entries
    **[M]**.
11. Several visible behaviours come from **script quirks**, which must be
    reproduced as behaviour, not code. Examples: `if (%InvSlot = -1)`@s:16423
    makes instant-use data always set, and BSD sections overlap by 1px per row
    because height math uses 96 while spacing uses 97.
12. Plant validation results (overlap, float, stuck, unstable, buried) come
    from engine-native `fxDTSBrick::plant()`. Scripts only map the result codes
    to HUD images (`%plantErrorCode`@s:5936). The rules themselves must be
    specified separately (04 §3).

## Regeneration

Run from the repo root on a machine with Python 3 and Pillow. `V20` is the
original install, which is only read.

```sh
V20="C:/Users/Maxwell/Desktop/Games/B4v21-Launcher-Release/versions/Blockland v20"
T=docs/research/ui-ux/tools; D=docs/research/ui-ux/data
python $T/gui_extract.py .research/v20-dso/client/ui/allClientGuis-Vanilla.gui .research/v20-dso/client/scripts/allClientScripts-Vanilla.cs --out $D/v20-static-objects.json
python $T/gui_outline.py $D/v20-static-objects.json $D/gui-layouts
python $T/profiles_table.py $D/v20-static-objects.json $D/gui-profiles.md
python $T/asset_manifest.py "$V20" $D/ui-asset-manifest.json $D/ui-asset-manifest.md .research/v20-dso/client/ui/allClientGuis-Vanilla.gui .research/v20-dso/client/scripts/allClientScripts-Vanilla.cs
python $T/bind_extract.py .research/v20-dso/client/scripts/allClientScripts-Vanilla.cs 15310 15496 > $D/default-binds-apply.tsv
python $T/defaults_compare.py .research/bl-decompiled/v20/client/defaults.cs "$V20/base/client/defaults.cs" $D/client-defaults-stock-vs-installed.tsv
python $T/function_index.py $D/v20-function-index.tsv .research/v20-dso/client/*.cs .research/v20-dso/client/scripts/allClientScripts-Vanilla.cs .research/v20-dso/client/ui/allClientGuis-Vanilla.gui .research/v20-dso/server/*.cs .research/v20-dso/server/scripts/*.cs .research/v20-dso/main-Vanilla.cs
python $T/dynamic_scenes.py $D/v20-static-objects.json content/stock-catalog-003/stock-catalog.json 1024x768 $D/v20-dynamic-scenes-1024x768.json
python $T/gui_render.py $D/v20-static-objects.json,$D/v20-dynamic-scenes-1024x768.json "$V20" docs/research/ui-ux/renders 1024x768 MainMenuGui escapeMenu Dyn_HUD_Bricks --annotate
python $T/verify_citations.py docs/research/ui-ux
```

`gui_render.py` produces **layout reconstructions**. They use the original
bitmaps, bitmap-array skins, button `_n` states, `mColor` tints and `.gft`
glyphs, placed with Torque's resize rules. They are not screenshots: 3D views,
scrollbar thumbs, popup text metrics, text wrapping and hover/pressed states
are simplified (04 §1.9). Use them as geometry and skin references, and compare
feel against Maxwell's memory.

## Verification

`tools/verify_citations.py` checks that every `c:/g:/s:/ms:/d:` citation is in
range, and that every `` `ident`@key:N `` claim finds the identifier within three
lines. The result of the last run is recorded at the end of
[05-fidelity-checklist.md](05-fidelity-checklist.md).
