# Doors: click swaps (2026-10-02)

Max asked whether the bundled door bricks work. They did not: the importer
flagged `isDoor`/`isOpen` as unsupported and nothing opened them. In v20
and v21 doors are Brick_Doors' script, which swaps a door's datablock to
`openCW`/`openCCW` (or `closedCW`/`closedCCW` when open) when clicked.

The engine gets a generic seam instead of a door rule (seams.md "Click
swaps"): a catalog entry may name `swap: { front, back }`, the brick a click
turns it into from in front of it (its -Z side) or from behind. Limits: the
targets must be bricks of the same catalog (its own Add-On), or the host's
`ToolCatalog` drops the swap; a target of another size leaves the brick as
it is; a brick swaps at most every 250 ms. The importer is
the policy: it maps `isDoor` fields onto `swap` (CW for the front, inferred;
the script that picks the side is not in hand). Brick_Doors' doors and the
Halloween coffins open and close.

Guards: `tool_catalog::tests::a_click_swap_stays_within_its_own_catalog`,
`special_bricks::a_click_swaps_a_brick_by_the_side_it_is_clicked_from`
and `installed::a_door_swaps_to_its_open_and_closed_bricks`.
