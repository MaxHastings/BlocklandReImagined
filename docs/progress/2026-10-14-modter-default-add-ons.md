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

## Default list

The six are `original` entries in `packages/default-addons.json`, on at
start, pinned to the sha256 of Maxwell's copies (E:\Downloads, 2026-10-14),
the brick packs in v20's Add-On name order between InvertedCorners and
MoreRounds, the print pack after Colorset_Trueno. The private Add-On bundle
must be rebuilt and uploaded (`python tools/addon_bundle.py upload --search
<folder with the zips>`) before a release carries them.

## Evidence

Imported with the branch's `bri-import-addon` against the main content and
the v20 reference: all six "converted"; 316 of 322 ModTer bricks placeable.
Left out: `4x CornerD` and `4x CornerD Inv.` (4xPack and 4xPackZ: one concave
collision piece, one empty piece) and siba's two streetlights (concave
collision), which `collision::bake` refuses rather than hull.

## Next

- Rebuild and upload the Add-On bundle with the six copies.
- Decide whether concave Add-On collision should hull per mesh, as Torque's
  convex collision did, to bring back the six left-out bricks.
