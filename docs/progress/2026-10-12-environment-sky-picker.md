# 2026-10-12 Environment window: Sky picker

Max asked to bring Steam Blockland's Environment Sky, Ground and Water
pickers over, the way the lighting settings already were. This adds Sky as a
picture grid in the Environment window's Simple page, on every map. Ground
and Water are left out, on Max's rule.

## Decisions

- **Sky, on every map.** The map bundle already converts every stock map's
  skybox, so the picker offers those (deduplicated: Blue2, Spooky1, Spooky3,
  Sunset, Skylands, Halloween, Slate Desert, Slate Storm Revised with its
  clouds) plus **Plain**, six white faces tinted by Sky Color, which stands
  in for Steam's White and Blank. No Steam asset is copied. Heat2 and Limbo
  exist only in Steam and are not offered.
- **Ground and Water only on flat maps (Max, 2026-10-12).** Steam's maps are
  a flat plane, so changing the ground or adding water fits there; on
  Bedroom or Slopes it makes no sense. No stock v20 map is a flat-plane
  world: their floors are terrain (Bedroom's and Kitchen's yards, Slopes,
  Tutorial) or modelled plates (the Slate maps). A first version that
  retextured any map's terrain was built and taken out again before merge.
  The picker model keeps a `ChoiceKind` so Ground and Water can join it for
  maps that declare a flat, replaceable ground.
- **Over the map's own.** `sky` is one more optional
  `atmosphere::Settings` value, like the lighting values. Unset draws the
  map's own. The host only checks it is a well-formed name; each client looks
  it up in its own content, and a name it lacks leaves the map's own. Reset,
  the picker's "Map's own" cell and a map change clear it; the Simple looks
  (Map Default, Overcast, Night, ...) set lighting only and keep the picked
  sky. A picked sky keeps the map's fog and lighting.
- Add-On scripts read and set it as a string through
  `environment()` / `set_environment` (`docs/modding/rules.md`).

## What changed

- `crates/content/src/atmosphere.rs`: `sky`, name validation.
- `crates/package-runtime/src/script.rs`: script access.
- `crates/net/protocol-changes/environment-sky.md`: wire note.
- `crates/render/src/environment_choices.rs` (new): reads the offered skies
  from the bundle, makes their thumbnails and swaps a scene's sky
  (`set_sky`). `scene_loader` now builds its sky through the same
  `append_sky`.
- `crates/ui`: `EnvironmentModel` choices, the Sky button and the new
  `AdminEnvironmentChoice` picker screen.
- `crates/client/src/app/scenery.rs` (new): loads the skies on a worker,
  uploads pictures, redraws the sky when the host's settings change, and
  resets on map or lighting reloads.
- `scene_snapshot`: `BRI_SKY` renders a pick offscreen.

## Evidence

Run in a Linux container with the content packs from Max's PC
(lavapipe for GPU tests):

- `cargo test -p bri-content atmosphere`, `-p bri-render --lib` (63),
  `-p bri-ui` (lib 221, `admin_screens` 19 including
  `the_sky_picker_picks_from_pictures`), `-p bri-sim --test script_api`
  (28), `-p bri-package-runtime`: all pass. `-p bri-client --lib`: 516
  pass; the one failure needs the vehicles content pack, absent from the
  container.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean;
  `cargo fmt --all`.
- Offscreen renders (`scene_snapshot` with `BRI_SKY`): Slopes with its own
  sky and Storm, Slate with Plain, Kitchen with Skylands. The window and
  the sky grid rendered offscreen look right.

`tools/gate.py` was not run (no Windows gate in the container); the PR's
Windows CI covers build, clippy and content-free tests.

## Limits and next

- Picking a sky re-uploads the map scene, which can hitch for a frame.
- Next, if Max wants it: maps declare a flat, replaceable ground (an
  authored map property, not a name check), a Steam-style flat map that
  does, and Ground and Water pickers that appear only there.
- For Max to try: open the Environment window as admin on any map, pick a
  few skies, change map (it returns to the map's own) and Reset.
