# 2026-10-12 The save list reads each save's header, not the whole file

Finding 7 of the load-path waste audit (`perf/waste-audit-2-load-paths.md`
in the project files). `Store::read_header` read every native save in full
(up to the 1 GiB build limit) to decode only its header, and the save list
does that for every save on every Load/Save Bricks open, after every save
and inside every load (about 43 MB on Max's PC today, growing with every
build saved). It now reads the magic, the header's length and the header;
saves from before the split header are still read whole.

Checks: new test `a_save_list_reads_only_each_saves_header` (a save grown
sparse past the limit after its header still lists; a cut header is still
an error); `cargo test -p bri-client --lib -- saves` and clippy
`-D warnings` on bri-client pass.
