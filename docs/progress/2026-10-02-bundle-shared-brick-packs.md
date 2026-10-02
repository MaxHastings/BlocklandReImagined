# Bundle: the brick packs both of Maxwell's saves use (2026-10-02)

Maxwell's rule for v0.1.13: bundle, switched on, only the brick packs that
define bricks both Halloween Block Party 2026 and Jazz CityRPG use. Sixteen
of the 17 such packs are now bundled originals in
`packages/default-addons.json`, credited from each zip's `description.txt`
and pinned to the sha256 of Maxwell's copy (his checkout's git-ignored
`content/Add-Ons`). None of their files is in the repository.

Same brick name in two packs: a save names each brick by its name, and the
definition loaded last wins, as in v20 (`crates/bls/src/bls.rs` builds the
lookup in load order; the base game loads first). The list's order is the
load order, so the brick packs are listed in v20's Add-On name order and
the last of them naming a brick wins: Filler over BlackDragonIV, and GIANT
over both. No pack is special-cased. `python tools/addon_bundle.py find`
now prints the names shared between listed originals and the base game,
and which wins.

Already stock: Brick_Halloween. v20 shipped it and our stock catalog
carries its five bricks (Gravestone and the four pumpkins, with pumpkin
carving keyed to the stock Pumpkin). Steam's copy adds twelve (coffins
and skulls) but declares those five again, so bundling it would hand the
saves' pumpkins to the Add-On's copy and lose carving. It is left out; the
twelve need the importer to keep a brick the base game already declares
(v20 redeclaring a datablock changes the same one). Brick_Christmas_Tree
also shipped with v20 but is not in our stock catalog, so it is bundled.
Brick_Doors and Brick_Window are v21 Add-Ons, not stock: bundled.

Guard: `defaults::tests::the_bundled_brick_packs_are_the_chosen_ones_in_v20_order`
fails if a brick pack is added or dropped without changing its list, is
off at start, has no pinned copy, or loads out of v20 order.

The Gate builds the bundle on the PC with
`python tools/addon_bundle.py find --search content\Add-Ons` (check each is
pinned) and then the release's `build`/`upload` with the same `--search`.
