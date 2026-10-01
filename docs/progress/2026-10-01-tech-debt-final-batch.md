# Tech debt final batch (v0.1.12)

Thread: Tech debt hot spots. Base: claude/batch176b 41c46fd1.

Done, each to cut the merge conflicts lanes kept hitting (plan in the
thread's tech-debt/plan.md):

- Shared dependency versions in `[workspace.dependencies]`; cargo-machete
  finds no unused dependency (bri-foliage-import no longer builds zip 2).
- Dead code, probes and old-format defaults removed, keeping every file
  v0.1.0 to v0.1.10 wrote loadable (checked against the release tags:
  world saves always wrote the fields; weapon saves and event checkpoints
  never reach disk).
- Importer ports: `ports/<port>/entry.json` per port instead of one
  `ports.json`.
- Script operations: one module per capability (`ops/<capability>.rs`),
  listed once in the union-merged `ops/list.rs`; `Op`, `capability()`,
  `name()` and the limits check are generated. `Op::X(ops::X { .. })`.
- Default Add-On tests read `packages/default-addons.json` instead of
  restating our own ids.
- Modding guide split into one page per topic.

Checks: `cargo check` and `clippy -D warnings` (Rust 1.93.1) clean on the
workspace; tests of package-runtime, addon-import, package, chaos
script_effects_fuzz and sim's package, script and duplicator targets pass.
Cloud failures were GPU-only (no adapter).

Left for the very end, with the app.rs split: one rustfmt commit (271
files drift today; doing it now would conflict with every lane in flight)
and a CI `cargo fmt --check`. Not done: host handlers per op family
(still match arms in `session/packages.rs`) and script bindings per
family; both are next if conflicts there continue.
