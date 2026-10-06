# Doors close the way they opened (2026-10-05)

Max reported that closing either Halloween coffin (standing or lying) opened
its back. Brick_Halloween's 2012 script gives each coffin one open datablock
that is both `openCW` and `openCCW`, with `closedCW` the plain coffin and
`closedCCW` a `falseBack` variant (the coffin with its back panel open).
The importer's door swap (2026-10-02, `click-swap-doors.md`) closed an open
door to `closedCW` from the front and `closedCCW` from behind, so a coffin
clicked from behind closed onto its false back.

Support_Doors, which picks the closed side in the original, is not in hand,
so the rule is inferred from the door data: the open datablocks name their
direction (`openCW`/`openCCW`), and Brick_Doors' glass door pairs
`brickDoorGlassOpenCCWData` with its own `closedCCW` mesh. An open door now
closes back the way it opened, from either side: to `closedCCW` only when it
is the `openCCW` datablock and not also `openCW`, otherwise to `closedCW`.
Opening still follows the clicked side. The coffins' false backs are no
longer reached by a click (their swaps still open them when loaded).

Guard: `installed::an_open_door_closes_the_way_it_opened` (hermetic, a
synthetic coffin-shaped and glass-shaped door). Shipping needs the bundled
Brick_Halloween re-imported (`python tools/addon_bundle.py build
--missing-ok`, then `install`, or the release bundle step).
