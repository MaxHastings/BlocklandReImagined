# 2026-10-12 Environment window: Sky, Ground and Water

Max asked to bring Steam Blockland's Environment Sky, Ground and Water
pickers over, with every Steam sky, ground and water ("I do not want to be
missing anything"). Sky works on every map; Ground and Water only on flat
maps (Max's rule: a new ground or a sea makes sense on Slate, not on
Bedroom or Slopes).

## Decisions

- **Steam's content is generated, never committed.** A new required
  package, `v20-environments` (`environments-001`, role `environments`),
  is built by `bin/environment_pack` from a Steam Blockland folder:
  `python tools/bootstrap.py --steam "<Steam Blockland>"`. Players never
  need Steam: the pack ships in the release zip and CI content like every
  other generated pack. Without `--steam`, bootstrap writes an empty pack,
  and the window offers the v20 skies only.
- **Every Steam choice.** 13 skies (Steam's order first, with their light,
  fog and rain or snow: Slate Storm's rain from its zip, Spooky3's snow),
  15 grounds and 4 waters (Brick, Default, Lava, White). The v20 map
  bundle's skies follow, minus those a Steam sky duplicates (same first
  face by sha256). A missing drop texture fails the import.
- **Flat is a property of the map, not its name.** `Scene::plate()` finds
  a plate: no terrain, static or datablock models, interiors whose first
  detail draws nothing, and a level collision face under the first spawn.
  Checked on every stock map: exactly 8 have one (the Slate family,
  Skylands, Construct and Destruct), all at top 0 and ±3584. Elsewhere the
  Ground and Water buttons read "Flat maps only".
- **A picked Steam sky brings its look**, as Steam's does (sun, ambient,
  fog, its rain or snow over the map's own); a v20 sky keeps the map's.
  Picking a ground or water resets its colours and height to the pack's.
  The Simple looks change lighting only and keep the scenery.
- **Water is real on the host.** A picked water replaces the map's own
  liquids with a sea from the plate top + height down 100 units, 1000 past
  the plate; `none` removes them. Lava kills as v21's
  `Armor::onEnterLiquid` does: 20 damage on entry, then every 300 ms,
  with a "Lava" damage type. Clients mirror it for prediction, splashes,
  the underwater tint and where rain stops.
- Settings keys `ground`, `ground_color`, `water` (`none` for no water),
  `water_height` (0 to 100), `water_color`, `underwater_color`, readable
  and settable by Add-On scripts (`docs/modding/rules.md`). Wire note:
  `crates/net/protocol-changes/environment-scenery.md`.

## Evidence

Linux container, content packs from Max's PC, a stand-in Steam folder for
the importer, lavapipe for renders:

- `cargo clippy --workspace --all-targets -- -D warnings` clean,
  `cargo fmt --all`.
- Tests pass in bri-content, bri-render (lib), bri-ui (lib and
  `admin_screens`), bri-admin, bri-package-runtime, bri-convert, bri-sim
  (`a_picked_water_floods_a_flat_map_and_lava_burns`: a crouched swimmer
  goes 100, 80, 65; `script_api` env keys) and bri-client weather
  (`a_steam_skys_snow_replaces_the_maps_rain_until_unpicked`).
- Offscreen renders with `scene_snapshot` (`BRI_ENV_PACK`, `BRI_SKY`,
  `BRI_GROUND`, `BRI_WATER`, `BRI_WATER_HEIGHT`): Slate with Sunset and
  Grass; Slate with Blue2, Plate and Default water at 1; Skylands with
  Heat2 and Lava; Sea Revised from underwater with Brick water.

## Limits and next

- Texture scrolling is not drawn; every Steam ground and water has scroll 0.
- Vehicles take no lava damage.
- The ground covers the plate only; the sea reaches 1000 past it.
- For Max: run `python tools/bootstrap.py --steam "S:\SteamLibrary\steamapps\common\Blockland"`
  once, then `python tools/ci_content.py upload`. Then, as admin, try each
  picker on Slate and Skylands, swim in Lava, and check Bedroom shows
  "Flat maps only" for Ground and Water.
