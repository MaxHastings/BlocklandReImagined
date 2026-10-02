# BLB lines read as v20 scans them (2026-10-02)

v0.1.13's packaged `--check` showed 16 bundled bricks with no geometry:
13 of Brick_Window's windows ("Invalid vector: 0.2 0.2 0.8", a vertex
colour written without alpha), its 2x4x5 window ("invalid float literal")
and Brick_Plant's 2X2petals and 5x6leaves ("Invalid attachment grid row",
a grid row shorter than the brick). The reader in `crates/convert/src/brick.rs`
demanded exact lines; v20 scans each with `%f` and reads a grid row's
`width` bytes, so it took all three.

Now each numeric line reads each word's leading number (`0.5f` is 0.5),
stops at the first word with none, and ignores numbers past the ones it
needs. A colour with three numbers is opaque (alpha 1; inferred, since an
unset alpha is not visible in the decompiled reader we have). A short grid
row is solid where it runs out (its terminating zero is "any other byte"
in v20's cell rule); a long one is cut to the width.

Guard: `brick::tests::lines_read_as_v20_scans_them` (made-up fixture; fails
on the old reader). `cargo test -p bri-convert` passes; clippy clean.

For the Gate: every stock BLB that converted before reads the same (the
change only accepts lines that used to fail), but check that the
regeneration's known geometry failure (`1x1x5spike.blb`) still fails as
expected, and that the next bundle shows Brick_Window and Brick_Plant with
no failed brick_geometry asset.
