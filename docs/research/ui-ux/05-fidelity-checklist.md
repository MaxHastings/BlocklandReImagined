# 05 — Prioritized fidelity checklist

Use it two ways: **implementation acceptance** (what Astra's code and tests
must show) and **Maxwell's playtest** (what to try and compare with memory of
v20). Evidence and citations are in files 01–04. Priority meanings:

- **P0**: required by the alpha contract, or its absence would be immediately
  wrong to a v20 player. Blocks the playtest handoff.
- **P1**: strongly characteristic behaviour. Missing it will be noticed during
  a thorough playtest.
- **P2**: polish, optional, or out of scope unless cheap.

"Rewrite status" reflects the repository on 2026-09-26 at about 20:30
(crates `content, convert, world, sim, net, render, physics`; no windowed
client or UI yet).

---

## P0 — contract-critical

**P0-1 Main menu recognisable** (01 §2)
- Build: title art, left column of text-art buttons with relative scaling,
  Quit bottom-left, About/Credits bottom-right, screenshot slideshow
  background with 2 s fade and 2 s hold, ascending note hover sounds, no
  music by default. Auth/demo/buy elements removed.
- Accept: layout test against `data/gui-layouts/MainMenuGui.md` rects scaled by
  the relative rule. Hover plays Note3…Note7 in the listed order.
- Playtest: "Does the menu look and sound like v20 at my resolution?"
- Rewrite status: not started.

**P0-2 Start Game / map select / loading** (01 §3, §5)
- Build: list sorted by map display name, preview from sibling png/jpg else
  "UNKNOWN MAP", description overlaid at the bottom of the preview, Single
  Player / LAN radios, blocker in Single Player, Launch → Connecting → Loading
  with the three phase texts and a full-screen map picture.
- Accept: Bedroom, Kitchen and Slopes appear with their original preview art
  and names. The loading phase strings match exactly.
- Playtest: start each map in Single Player and LAN host.

**P0-3 Join LAN / Direct IP** (01 §4)
- Build: Join screen with Query LAN (list), Connect to IP dialog (IP +
  password), Connecting dialog with Cancel. Hide Internet.
- Accept: loopback multi-client test drives the same code path as the dialog.
- Playtest: second PC joins by LAN list and by IP.

**P0-4 In-game HUD** (01 §6)
- Build: runtime brick bar (10 slots, 64 px cap, slot numbers, paint-tinted
  icons, active frame, name bar with both tooltips), paint box (division
  columns + FX column, fading, can label, "Division - n" / "FX - name"), tool
  box (5 slots, Hammer/Wrench/Printer, name/"Q = tools"), slide
  in/out timings (10×10 ms; tools out 20 steps; paint in instantly),
  crosshair, plant-error images for 800 ms, center/bottom print, chat HUD.
- Accept: offscreen render at 1024x768 and 1920x1080 compared against
  `renders/Dyn_HUD_*` geometry. The unit test for the icon-size formula is
  `min(64, floor(W/10))`.
- Playtest: switch 1/Q/E/wheel repeatedly, and watch the slide behaviour and
  labels.

**P0-5 Controls & first-run scheme** (02 §1)
- Build: first-run mouse/keyboard dialog with Two Button + Wheel and Standard
  Keyboard preselected. Keymaps exactly as §1.1–1.3. Remap list with stock
  categories and names. Global Esc / ~ / Alt+Enter.
- Accept: a table-driven test asserts every bind in `data/default-binds-apply.tsv`
  for each of the 2 × 3 scheme combinations.
- Playtest: build a small wall with the numpad only, then with the laptop
  layout.

**P0-6 Build interactions** (02 §3–4)
- Build: scroll-mode state machine. Click-to-deploy ghost (about 15-unit
  reach). Shift relative to **player facing**, snapped to the dominant world
  axis (0.5 per stud, 0.2 per plate, up/down = 3 plates, separate 1/3 keys).
  Super-shift by footprint. Rotate quarter turns with parity re-centering.
  Plant keeps the ghost. Key repeat 200 ms / 50 ms, including plant. Cancel.
  Undo stack (plant/paint/print). Plant errors (TooFar 50 + radius, Overlap,
  Float, Stuck, Unstable, Buried, Limit).
- Accept: sim tests for each shift/rotate case, facing N/E/S/W, both parities,
  and the repeat timing. Placed positions stay on the 0.25/0.1 lattice
  observed in all 276,612 saved bricks.
- Playtest: rotate a 1x2 four times and watch whether it drifts as you
  remember (v20's re-centering depends on facing, s:5606–5634). Hold numpad 8
  and watch the repeat speed.
- Rewrite status: `bri-sim::ghost` implements shifts with a camera-derived
  cardinal direction and a *symmetric* rotation correction, described in-code
  as a deliberate replacement for "incomplete camera branches". The recovered
  v20 branches are complete (one branch is the identity case). v20 uses the
  player's body forward vector, which differs from the camera during free
  look. **Decide whether to match v20 exactly.** Plant reach 50 + radius
  matches v20.

**P0-7 Wrench + events subset** (01 §12–13)
- Build: wrench (normal, sound, vehicle-spawn variants) with Copy locks,
  sorted popups (lights unsorted), `NONE` first, Send/Cancel/Events, left-shift
  suppression. Events dialog with enable, delay (0–30000 ms clamp), input,
  target, named-brick sub-popup, output, typed parameters, trailing empty row,
  row deletion, and send-then-close-both.
- Contract subset mapped to stock names: inputs **onActivate**,
  **onPlayerTouch**. Targets **Self**, **<NAMED BRICK>** (owner's names).
  Outputs **setColor** (paintColor), **setRendering** (bool), **setColliding**
  (bool), **setRayCasting** (bool), **setLight** (datablock FxLightData),
  **setEmitter** (datablock ParticleEmitterData), **setColorFX** (list 0–6).
  This matches `bri-world::Action` one-to-one. Cheap additions that complete
  "emitter" and "light" behaviour: `setEmitterDirection` (list),
  `setShapeFX`, `fireRelay`, `onRelay`, `setEventEnabled`/`toggleEventEnabled`,
  `cancelEvents` **[M]**.
- UI policy for unsupported stock events **[M]**: recommended to list only
  supported inputs and outputs in the popups, and show imported unsupported
  rows as read-only with their original names, so saves round-trip without
  silent loss.
- Accept: dialog round-trip test (open brick → rows filled → send → server
  state equals). Named target invalidation closes the dialog.
- Rewrite status: the event model and scheduler exist in `bri-world`. No UI.

**P0-8 Brick selector & favorites** (01 §10)
- Build: tabs in datablock order, subcategory sections, 6-wide 96 px tiles on
  a 97 pitch, name label, cart of 10, the click/click-again/swap/clear rules,
  right-click instant use, DONE/Enter/B-again buys, Esc cancels, favorites
  1–0 with Set Favs (stored by **name**), 50% alpha on empty favorites, and the
  first-spawn auto-load of favorites 1.
- Accept: a golden test of tab order and section order against
  `data/v20-core-brick-datablock-order.tsv`. Favorites save/load round-trip
  by uiName.
- Playtest: build a cart, swap slots, save to 3, reload 3, right-click a ramp.
- Content gap: the stock catalog lacks the default-enabled add-on bricks
  (03 §7). Decide the stock definition **[M]**.

**P0-9 Tools: hammer, spray can, printer, wrench** (02 §5)
- Build: ranges (hammer 5 / 5.5 when looking down, wrench 10, printer 10,
  brick activation 5 within a 10-unit ray), paint projectile, FX cans, print
  selector with letter accelerators, trust checks.
- Rewrite status: `bri-sim::session` uses a single 8-unit edit/removal reach.
  Split it per tool to match v20.

**P0-10 Avatar customization** (01 §9)
- Build: 3×5 part grid, part pickers, color grid from the avatar colorset,
  symmetry, clan tags, LAN name, Randomize, favorites 0–9, Done applies and
  closes Options, X/Esc reverts, 3D preview.
- Accept: applied appearance replicates to the other client. Revert restores
  every field.

**P0-11 Escape menu, Options, player list, chat** (01 §6.5, §7, §8, §14)
- Build: colored escape menu with the same button order (disable
  out-of-scope items), confirm dialogs with v20 wording. Options tabs with at
  least Controls (remap + checkboxes), Graphics (resolution/fullscreen/HUD
  toggles/chat size), Audio (volumes + toggles). Player list (F2). Chat T/Y,
  `/commands`, 6.5 s fade, PageUp/Down, v20 name coloring.
- Ownership on LAN **[M]**: stock v20 trusted everyone on LAN. The contract
  requires enforcement, so document it as an intentional change in the
  playtest guide.

**P0-12 Save / load dialogs** (01 §15)
- Build: v20 dialog layouts over the rewrite's authoritative save and load.
  Import original BLS from `saves/<map>/`.
- Decision **[M]**: client-side (v20) versus host-side saving.

## P1 — characteristic behaviour

| ID | Item | Evidence |
|---|---|---|
| P1-1 | UI fonts from `.gft` caches (Impact buttons/windows, Palatino chat, Arial labels), text outline rules | 03 §3 |
| P1-2 | Bitmap-array window chrome, button state quartets, `mColor` tints, button art stretched to rects | 04 §1.2–1.3 |
| P1-3 | Zoom and free look are hold actions. Wheel adjusts zoom FOV 5–85 | 02 §2 |
| P1-4 | Tab 1st/3rd person with smooth transition; Fast Switch option | 02 §2 |
| P1-5 | Walk (C) = 0.4 multiplier applied immediately to held keys | 02 §2 |
| P1-6 | Super-shift smart toggle (Alt hold > 200 ms reverts) + badge | 02 §4.4 |
| P1-7 | Pressing the active slot's number deselects. Empty-slot fallback and "You don't have any bricks!" message | 02 §3 |
| P1-8 | HUD brick icons tinted with the paint color (red at start) | 01 §6.1 |
| P1-9 | Player animations for shift/rotate/plant/undo/activate (thread 3) | 02 §4.2 |
| P1-10 | Brick move/plant/rotate/change sounds, gated by audio prefs | 03 §4 |
| P1-11 | Print selector letter accelerators, including shift-symbols | 01 §11 |
| P1-12 | Wrench popups and event datablock parameters from **default-enabled add-ons** (lights, emitters, items, sounds, vehicles) | 03 §7 |
| P1-13 | Chat: speaker talk animation for strlen×50 ms, typing indicator, curse filter pref | 01 §6.5 |
| P1-14 | Respawn countdown center print and "Click to respawn." | 01 §6.4 |
| P1-15 | F5 toggles names + crosshair. Crosshair visibility in 3rd person **[M]** | 04 §1.6 |
| P1-16 | Colored escape menu toggle (Advanced → Colored Escape Menu) | 01 §7 |
| P1-17 | Main-menu hover notes and escape-menu hover notes | 01 §2, §7 |
| P1-18 | Loading screen accepts chat. Esc disconnects (stock) or opens the escape menu (B4v21) **[M]** | 01 §5 |

## P2 — polish / optional

- Temp-brick flash settings (time, range, offset, inside/outside colors) in Advanced.
- Build macros (Ctrl+Numpad 0 / Ctrl+Numpad Enter).
- Network tab (replace with an informational panel).
- Mini-games, admin panel, ban lists, server config, Add-Ons/Music lists.
- Screenshot keys (Ctrl+P, Shift+P, DOF).
- Reproducing BSD's 1 px section overlap and the unsorted light list (only if exactness is wanted).
- B4v21 extras: FOV slider, wider player list, tab arrows in the BSD, key repeat in text boxes.

## Maxwell's playtest script (UI/controls portion, about 20 minutes)

1. Fresh profile: confirm the first-run control dialog, choose Two Button + Wheel / Standard.
2. Main menu: hover every button (notes), open Player → change hat and colors,
   press Esc (reverts?), change again, press Done (applies, returns to menu).
3. Start Game → Bedroom → Single Player → Launch. Watch the loading texts.
4. In game: 1–0, wheel, Q, E, E again (next column), wheel in paint, and check
   the HUD slides.
5. B → Bricks tab → add 3 bricks by double-click, swap two cart slots, clear
   one, Set Favs → 3, DONE. Then B → 3 → DONE.
6. Place: click to deploy, numpad 8/2/4/6 facing each compass direction, +, 5,
   3, 1, 9, 7, Enter (hold it), 0, Ctrl+Z. Try super-shift with Alt held and
   tapped.
7. Plant errors: overlap, float, and too far (walk more than 50 units away).
8. Hammer a brick, paint a brick, print a print brick with letters via
   keyboard, and wrench a brick: set name "door", Copy-lock the name, wrench
   another brick (name kept?).
9. Events: onActivate → Self → setColor, onActivate → <NAMED BRICK> door →
   setRendering after 1000 ms, onPlayerTouch → Self → setColliding. Send,
   click, walk on.
10. F (hold zoom, wheel), Z (free look), Tab, C (walk), T chat with /sit, F2
    list, Esc menu layout.
11. Second player via LAN list and via Connect to IP: chat colors, ownership
    (hammering the other player's brick).

Record each step as ✓ same / ≈ close / ✗ different, with a note, per
`docs/research/ui-ux` item IDs.

## Verification record

(Filled in by `tools/verify_citations.py`; see the end of this file.)
