# Brick grids read as v20 reads them

## What changed
- SPECIALBRICK: v20's loader (`blocklandv20.exe` 0x53c052, `strncmp(line,
  "SPECIALBRICK", 12)`) fills the grid with the same loop as BRICK (0x53c06e
  against 0x53ac15): every cell solid, with studs up and down. The converter
  already gave these seven stock files the BRICK grid; the "requires review"
  warning is gone and `docs/content-conversion.md` says why.
- SPECIAL grid rows (0x53c1d0): `-` is empty, `u`/`b` set the up grid,
  `d`/`b` the down grid, and any other byte is solid with no studs. The
  converter used to lower-case rows, so an upper-case `U`, `D` or `B` gained
  studs v20 never gave it, and any other letter failed validation. Rows now
  map exactly as v20 reads them.
- COVERAGE is already used: `crates/client/src/brick_cover.rs` culls faces
  hidden by a neighbour (test `neighbours_hide_covered_faces_as_v20_coverage_does`
  in `world_chunks.rs`). Nothing to add there.

## Tests
- `bri-convert` `special_grids_read_cells_as_v20_does`,
  `specialbrick_grid_is_the_brick_grid` (both fail on the old converter).
- `bri-sim` `grid::every_cell_pair_places_and_joins_as_v20_reads_the_grid`:
  all 25 pairs of `b u d x -`, side by side (overlap) and stacked both ways
  (join only up onto down).

## Not done
- v20 refuses a BLB with more than 10 collision boxes (0x53c3ac); we allow
  1024. Kept as a modding superset.
