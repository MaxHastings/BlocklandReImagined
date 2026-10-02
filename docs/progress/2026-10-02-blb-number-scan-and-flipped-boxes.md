# BLB numbers scanned like v20; flipped collision boxes

The packaged check still failed four bundled bricks:

- Brick_Domes `1x1x2Dome` (`0.7000000.700000 0.200000`) and
  `3x3x3DomeInverted` (`0.350000-0.350000-1.000000`): numbers written with no
  space between them. v20's `%f %f %f` scan starts each number where the last
  one stopped, so these read as 0.7 0.7 0.2 and 0.35 -0.35 -1. The reader now
  scans a line the same way (`c_float` is a small `strtod`) instead of word by
  word.
- Brick_VerticalPlatePack `*VerticalPrintCorner`: the second collision box is
  written with a negative width (`-0.4 1 5`). A box spans its center plus and
  minus half its size, so the reader takes each size's magnitude.
- Brick_Round_Corners `2x2x6roundCornerWall` (`NORMALS` with no colon) was
  already covered by the colon-optional headers in 6b380969.

All four sample files from the Gate convert. Guards in
`brick::tests::lines_read_as_v20_scans_them` (CC0 lines in our own words) fail
on the old reader. Also merged main b0f1dfd5: `CatalogEntry` carries both the
hole-brick `bot` and the door `swap`, and the importer's gap note names
`isBotHole`/`holeBot` only without a bot and `isDoor`/`isOpen` only without a
swap.

Checks: `cargo clippy --workspace --all-targets -D warnings`; tests of
bri-convert, bri-addon-import and bri-content.
