# 2026-10-01 Tech debt hot spots: progress entries and seam owners

Max asked to find what keeps slowing the parallel lanes down and remove that
debt in v0.1.11. Survey of the 118 merge commits on main since 2026-09-28
(files changed on both sides of each merge): docs/progress.md 95,
crates/client/src/app.rs 59, docs/modding/README.md 24, sim session.rs 11,
render scene.rs and scene.wgsl 11 each, package defaults.rs 11,
default-addons.json 10, package-runtime script.rs 10, session/packages.rs 9,
ops.rs 8 (progress.md already merges with the union driver, so its count
rarely meant a real conflict; app.rs and the shared lists did). The protocol VERSION line was hand-renumbered for every network
lane. The full findings and ranked plan are in the project's
tech-debt/plan.md.

First two fixes, which touch nothing a lane is editing:

- Progress entries are one file each in `docs/progress/` (this is the
  first), so entries no longer interleave under the union merge. docs/progress.md keeps the history and is only appended to, so
  entries already written there on older branches still merge.
- `docs/architecture/seams.md` lists each engine seam being built and the
  thread that owns it, so a second thread builds on it instead of starting
  its own (as happened with datablock reading, the ammo HUD, own-model
  loading and textured icons).

- The protocol version counts files in `crates/net/protocol-changes/`
  (build.rs) on top of the frozen 69. A wire change adds one file; nobody
  edits the number. `tools/gate.py` refuses a push that removes one.
  Evidence: `cargo test -p bri-net --test protocol_changes` passes, and adding
  a probe file rebuilt the list with it (version 70), removing it went back.
- `rust-toolchain.toml` pins Rust 1.93.1, what CI (`dtolnay/rust-toolchain@1.93`)
  and the PC use. The cloud's Rust 1.97 failed clippy on untouched code
  (vehicles world.rs `manual_checked_ops`); with 1.93.1,
  `cargo clippy -p bri-net --test protocol_changes -- -D warnings` is clean.

Next: the registries, one default Add-On list and the modding docs split, in
the final structural batch with the app.rs split.
