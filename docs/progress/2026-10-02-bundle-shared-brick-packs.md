# Bundle: the brick packs both of Maxwell's saves use (2026-10-02)

Maxwell's rule for v0.1.13: bundle, switched on, only the brick packs that
define bricks both Halloween Block Party 2026 and Jazz CityRPG use. All 17
such packs are now bundled originals in
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

Brick_Halloween: v20 shipped it, and our stock catalog carries its five
bricks (Gravestone and the four pumpkins, carving keyed to the stock
Pumpkin). Steam's copy declares those five again and adds twelve coffins
and skulls. In v20 declaring a datablock again changes the same one, so
the importer now keeps a brick the installed game already has as the base
game's (`Reference::base_bricks`; marked consumed in the report) and adds
only the new ones. Halloween is bundled: the twelve come in, the stock
pumpkins keep carving. Guard:
`installed::a_base_brick_declared_again_stays_the_base_games` (fails on
the old importer, which added the base brick again). Brick_Christmas_Tree
also shipped with v20 but is not in our stock catalog, so it is bundled.
Brick_Doors and Brick_Window are v21 Add-Ons, not stock: bundled.

Guard: `defaults::tests::the_bundled_brick_packs_are_the_chosen_ones_in_v20_order`
fails if a brick pack is added or dropped without changing its list, is
off at start, has no pinned copy, or loads out of v20 order.

The Gate builds the bundle on the PC with
`python tools/addon_bundle.py find --search content\Add-Ons` (check each is
pinned) and then the release's `build`/`upload` with the same `--search`.
