# 2026-10-14 Modular Terrain (ModTer) bundled and on by default

Maxwell asked for the six ModTer Add-Ons in the game's defaults for v0.3.1:
`Brick_ModTer_BasicPack`, `Brick_ModTer_InvertedPack`, `Brick_ModTer_4xPack`,
`Brick_ModTer_4xPackZ`, `Brick_ModTer_siba` and `Print_ModTer_Default`. His
friend's note: ModTer uses the same mapping as Blockland's ramp bricks.

## What ModTer needs

Every ModTer face is `TEX:PRINT` with `printAspectRatio = "ModTer"`, and the
BLB UVs run past 0..1 (a 64x face repeats its print eight times). The look
comes from the prints in `Print_ModTer_Default` (`brickRAMP`, `brickTOP`,
grass, dirt, rock, water, snow...). Our print overlay already samples with the
repeating sampler on the brick's own UVs, the path TEX:RAMP takes with
`brickRAMP`, so no renderer change was needed: what was missing was the
prints themselves and the bricks' collision.

- **Add-On print packs.** Import Add-On now turns a `Print_<aspect>_<name>`
  Add-On's `prints/*.png` (each with its `icons/<stem>.png`) into
  `assets/prints.json` (`bri_content::brick_materials::PrintPack`, new
  package kind `prints`), ids and aliases as the stock print converter makes
  them (`print/print_modter_default/grass`, `ModTer/Grass`). The client and
  the dedicated server merge enabled packs after the base game's prints
  (`Bundle::with_print_packs`, `content_identity::brick_materials_with`); a
  pack repeating a loaded package or print keeps the first, and one that does
  not validate is left out whole and said on the console. The print selector
  gets a ModTer tab with the pack's own icons. New planted ModTer bricks
  still default to `Letters/A`, as v20's `serverCmdBuyBrick` does.
- **Collision.** ModTer slopes set `collisionShapeName`. Add-On import baked
  every brick with no external shape, so these failed ("Brick needs external
  collision data") and never reached the brick menu: 4xPack and BasicPack
  imported with zero placeable bricks. Import now bakes a brick with its
  converted collision shape, as `stock_catalog` does for base bricks.
- **4xPackZ** picks each brick's category with an expression on
  Brick_Modified_ModTer (not bundled), which the catalog reader refuses. Port
  `brick_modter_4xpackz` declares the twelve bricks under Baseplates, as v20
  loads them without that Add-On; the importer no longer reports the
  replaced declarations as refused or duplicated.
- **Concave collision** (Max: "of course mod ter goes in", so all of it).
  `collision::bake` refused an authored collision piece that is not convex.
  It now splits such a piece with V-HACD (parry3d, already in the lockfile
  through rapier) and makes each part the exact convex hull of the authored
  points in it, so an inside corner stays open instead of being filled by
  one hull (Torque's own convex collision would have filled it). Pieces
  with no volume (flat leftovers in 4x CornerD Inv.) are dropped. Convex
  pieces are unchanged, so stock bricks bake as before. This applies to
  every brick with concave authored collision, Add-On or stock.

## Default list

The six are `original` entries in `packages/default-addons.json`, on at
start, pinned to the sha256 of Maxwell's copies (E:\Downloads, 2026-10-14),
the brick packs in v20's Add-On name order between InvertedCorners and
MoreRounds, the print pack after Colorset_Trueno. The private Add-On bundle
must be rebuilt and uploaded (`python tools/addon_bundle.py upload --search
<folder with the zips>`) before a release carries them.

## Evidence

Imported with the branch's `bri-import-addon` against the main content and
the v20 reference: all six "converted", all 322 ModTer bricks placeable
(316 before the concave split; 4x CornerD now bakes to 25 convex pieces,
siba's streetlights to 20 and 42). `bri-client --check` on a hard-linked
copy of the content with the six installed: 963 -> 1285 brick definitions,
Add-On health clean. An offscreen render (a local probe test, not
committed) drew the ModTer prints tiled across 4x and 16x shapes and tinted
by paint, and the print selector's ModTer tab listed 22 prints with icons.

## Next

- Rebuild and upload the Add-On bundle with the six copies.
- Maxwell's playtest of the ModTer slopes and corners (walking up them).
