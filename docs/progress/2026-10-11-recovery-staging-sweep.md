# 2026-10-11: a host clears staging files a killed writer left

Found by the world-safety suite: a writer killed mid-snapshot (SIGKILL,
TerminateProcess, power loss) leaves `.<slot>.<pid>.<n>.tmp` beside the
recovery slot, and nothing removed it.

- `bri_files::sweep_stale(path, min_age)` removes staging files named for
  that target and untouched for `min_age`; a write in progress is younger.
- A host calls it when it starts keeping its recovery slot, with 10 minutes
  (a snapshot takes seconds).
- Tests: `bri-files` unit `sweep_removes_only_old_staging_files_of_that_target`;
  `crates/net/tests/recovery_sweep.rs` runs a real host and checks the old
  file goes while a fresh one and another target's file stay.
- Checked on Linux: those tests, clippy `-D warnings` for bri-files and
  bri-net. Windows is left to CI.
