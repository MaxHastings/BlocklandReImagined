# 2026-10-12 Host, Join and Change Map share map-independent content loads

Finding 5 (and half of 4) of the load-path waste audit
(`perf/waste-audit-2-load-paths.md` in the project files).

Every Host, Join and Change Map rebuilt content that depends on the packs,
not the map: the brick definitions (every brick mesh read, every collider
built; on a local host both the host worker and the client's map
preparation did it), every brick surface and print image (read, SHA-256'd
and PNG-decoded), the brick geometry assets and the weapons content (read
and hashed a second time per Host through `map_content()`).

`ContentPaths` now carries `shared` loads built once per content load and
shared by its clones: `map_content()` (weapons, brick geometry and, through
`MapContent::definitions`, the definitions) and `materials()`. Loading
content again (Add-On changes) resolves new paths, so nothing outlives the
packs it came from. Hosting and joining still check the weapons and item
physics on disk against startup's.

Checks: new test `shared_loads_are_built_once_per_content` (moves the
packs away after the first load; the second answers from what was kept).
`cargo clippy -p bri-net -p bri-client --all-targets -- -D warnings`
clean, `cargo test -p bri-client --lib` and `cargo test -p bri-net` pass.
Not measured on Max's PC: it comes out of LOADING MAP (1.4 s warm) on
every Host after the first and out of every Join and Change Map.
