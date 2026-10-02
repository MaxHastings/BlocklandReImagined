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

Bootstrap's bundle build (`install_originals`) passes no `--search`, so on
the PC it missed the Tier reference copies a release build is pointed at
and only warned. `addon_bundle.py` find/build/upload now remember the
`--search` folders in `<content root>/_regeneration/addon-search.txt` (as
bootstrap remembers the v20 folder) and reuse them when none is given, and
always search the content root's `Add-Ons` drop folder. One release build
with the usual `--search` folders is enough; later bootstraps match it.

Third save (2026-10-02): Max counts 2023 XMas as a popular save too; the
rule is now packs used by at least two of the three. The rescan
(`/mnt/project-files/bundle-list-3saves.md`) adds Brick_Fence, Brick_Wedge,
Brick_InvertedCorners, Brick_Pole and Brick_Round_Corners (credited from
their description.txt, pinned), 22 packs in v20 order.

Two-of-three round (2026-10-02): the PC's missing-bricks thread downloaded
21 more packs; `/mnt/project-files/shared-bricks/bundle-list-2of3.md` lists
45 that qualify. The 43 brick packs are bundled (the two bots go to the
bot lane), in v20 order. The new packs' titles and authors are from their
description.txt (read on the PC); Brick_Default_Fence_Extras has none, so
it is titled by its folder and credited "Unknown".
