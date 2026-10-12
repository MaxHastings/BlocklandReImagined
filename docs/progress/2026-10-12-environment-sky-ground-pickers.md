# 2026-10-12 Environment window: Sky and Ground pickers

Max asked to bring Steam Blockland's Environment Sky, Ground and Water
pickers over, the way the lighting settings already were. This adds Sky and
Ground as picture grids in the Environment window's Simple page. Water is
left out for now.

## Decisions

- **Sky.** The map bundle already converts every stock map's skybox, so the
  picker offers those (deduplicated: Blue2, Spooky1, Spooky3, Sunset,
  Skylands, Halloween, Slate Desert, Slate Storm Revised with its clouds) plus
  **Plain**, six white faces tinted by Sky Color, which stands in for Steam's
  White and Blank. No Steam asset is copied. Heat2 and Limbo exist only in
  Steam and are not offered.
- **Ground.** v20 maps have no Steam-style ground plane: their floors are
  terrain (Bedroom's and Kitchen's yards, Slopes, Tutorial, ...) or modelled
  geometry. The Ground picker retextures the map's terrain with one of the
  stock terrain textures the bundle already converts. On maps without terrain
  it is inactive and reads "No terrain". It changes only how the terrain
  looks; collision is untouched.
- **Water.** Not now. v20 has one water texture, and Steam's water adds a
  plane at any height (and Lava kills), which changes play and physics rather
  than looks. It would be its own change.
- **Over the map's own.** `sky` and `ground` are two more optional
  `atmosphere::Settings`, like the lighting values. Unset draws the map's
  own. The host only checks they are well-formed names; each client looks the
  name up in its own content, and a name it lacks leaves the map's own.
  Reset and the picker's "Map's own" cell clear them; the Simple looks
  (Map Default, Overcast, Night, ...) set lighting only and keep the picked
  sky and ground. A picked sky keeps the map's fog and lighting.
- Add-On scripts read and set them as strings through
  `environment()` / `set_environment` (`docs/modding/rules.md`).

## What changed

- `crates/content/src/atmosphere.rs`: `sky`, `ground`, name validation.
- `crates/package-runtime/src/script.rs`: script access.
- `crates/net/protocol-changes/environment-sky-ground.md`: wire note.
- `crates/render/src/environment_choices.rs` (new): reads the offered skies
  and grounds from the bundle, makes their thumbnails, swaps a scene's sky
  (`set_sky`) and covers terrain (`grounded`, via
  `TerrainScene::with_ground`). `scene_loader` now builds its sky through the
  same `append_sky`.
- `crates/ui`: `EnvironmentModel` choices, the Sky and Ground buttons and the
  new `AdminEnvironmentChoice` picker screen.
- `crates/client/src/app/scenery.rs` (new): loads choices on a worker,
  uploads pictures, redraws the sky or terrain when the host's settings
  change, and resets on map or lighting reloads.
- `scene_snapshot`: `BRI_SKY` and `BRI_GROUND` render a pick offscreen.

## Evidence

Run in a Linux container with the content packs from Max's PC
(lavapipe for GPU tests):

- `cargo test -p bri-content atmosphere`, `-p bri-render --lib` (64),
  `-p bri-ui` (lib 221, `admin_screens` 19 including
  `the_sky_and_ground_pickers_pick_from_pictures`), `-p bri-sim --test
  script_api environment`, session environment tests, `-p
  bri-package-runtime`, `-p bri-net` replication: all pass.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean;
  `cargo fmt --all`.
- Offscreen renders (`scene_snapshot` with `BRI_SKY`/`BRI_GROUND`): Slopes
  with its own sky, Storm, Sunset with sand, grass; Slate with Plain;
  Kitchen with ice and Skylands (its floor is interior geometry, so only the
  yard outside changes). The window and both grids rendered offscreen look
  right.

`tools/gate.py` was not run (no Windows gate in the container); the PR's
Windows CI covers build, clippy and content-free tests.

## Limits and next

- Ground shows best on Slopes and Tutorial; on Bedroom and Kitchen it only
  changes the yard.
- Picking re-uploads the map scene (sky) or terrain (ground), which can hitch
  for a frame.
- Ground labels are texture file names (`TTgrass01`), and Slopes' detail
  texture is offered as `detail1`.
- For Max to try: open the Environment window as admin on Slopes, pick a few
  skies and grounds, change map (it returns to the map's own) and Reset.
