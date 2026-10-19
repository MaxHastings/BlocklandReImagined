# 2026-10-19 Test build size: why the gate builds cold

Max asked where about 50 minutes of compile and link time in the full suite
goes, and whether folding test binaries would fix it.

## Evidence

- PC gate logs `b138622a993d.log` and `a4b55e6dc601.log` (2026-10-17/18): both
  show `Compiling` for all 670 units and `Finished dev ... in 17m 19s` /
  `18m 06s`; the phase line of the first reads `build 1041s, clippy 352s
  (alongside), tests 1178s, retries 96s, total 2324s`. The warm gate was
  81-267 s of build on 2026-10-07 (`2026-10-07-gate-speed.md`). So a gate
  run is 39 minutes, of which the build is 17 and is a cold rebuild.
- `.bri-gate/target/debug/deps` (first 2,000 entries of the listing): 8.5 GB
  of test executables (282) and **17.6 GB of PDBs**, 27.5 GB in all and
  cut off, against `TARGET_DEPS_CAP` of 40 GB. Every bri-client test
  executable is 138-148 MB on Windows and links the whole client.
  `trim_target` wipes the whole target dir past the cap, so about every
  second gate run starts cold. (That the wipe fires is inferred: its message
  goes to stdout, not to the log.)
- Cloud (Linux, lld, 4 cores), per bri-client test target: link plus test
  crate 2.5-3.4 s at `debug = false`, 4.7 s with `line-tables-only`; the
  executable is 200 MB against 376 MB. Eighty-five such targets are ~4 CPU
  minutes here. The Windows linker and PDB writer are slower; not measured.

## Change

- `[profile.dev] debug = false` (was `line-tables-only`): no PDBs, half the
  executable size. Panics still print file:line; backtrace frames lose
  function names. Release keeps its line tables.
- `tools/gate.py`: past the cap, `prune_stale_artifacts` deletes superseded
  test executables and PDBs (keeping the newest per target) before it
  considers wiping the target dir. Test in `tools/test_gate.py`.

## Not done, and why

- **Folding bri-client's 85 test files into one binary** is deferred. The
  disk problem it solves is solved by the two changes above; the link time
  it saves is ~85 x 3 s here and unmeasured on Windows. Each file's tests
  already take turns on one GPU device (`support/gpu.rs`) and some own a
  fixed port, so a folded binary would have to be run one module per
  process by the gate (list the tests, group by module, `--exact`) to keep
  today's isolation. That is a gate rewrite plus ~90 file moves with no way
  to run the result in the cloud. Measure first: run
  `cargo test --no-run -p bri-client --timings` once on the PC with and
  without this change and read the link share of each target.
- **A shared shader/pipeline cache**: every App boot builds its pipelines
  (`create_render_pipeline` at 17 sites in bri-render/bri-client); tests
  within a binary share one `Headless` device but each boots its own App.
  Caching the pipelines per device would need an engine change that can only
  be proved on a GPU. wgpu's `PipelineCache` is Vulkan-only, so on DX12 only
  the driver's own shader cache helps. Measure first: time `App::load` in a
  render test on the PC.
