# 2026-10-07 Four tests that failed only under the gate's load

Four tests passed alone and failed in the PC gate, each because a deadline
counted time a loaded machine stretches. Same claims and bars, measured
properly.

## Changes

- **Bot think time** (`bot_think_time_16`): `Session::bot_think_nanos` is
  now the ticking thread's CPU time in `step_bots`, not elapsed time. The
  thread-CPU clock moved from `hardening_packages.rs` into
  `bri_sim::cpu_time` (its test copy deleted; `windows-sys`/`libc` are now
  normal target dependencies of `bri-sim`). Bar unchanged (15000 us debug).
- **Shared wait** (`crates/client/tests/support/wait.rs`): `until` counts
  game time only across steps that began with every app's scene pipelines
  compiled. A GPU opened after entering blocks its first frame on the
  compile while the hosted server ticks on; those ticks are no longer
  spent from the budget. A compile still running after `STALL` fails.
  New `until_done` waits for work off the game clock (a worker's bake or
  compile) with only the `STALL` guard.
- **app_item_render** `settle_lighting`: waits with `until_done` (the
  pipeline compile and the lighting bake are worker jobs, not game time).
  Before, the 45 s game budget was spent while they ran.
- **night_qa** brick-hand tests: the two copies of the setup became
  `host_holding_brick`, which waits with the shared `until_one`: loading is
  judged by its progress, then 120 s of game time to stand and 10 s to get
  the brick in hand (the old wall-clock numbers, now game time).
- **stresslab_flow**: its own 300 s wall-clock `until` is replaced by the
  shared wait (30 s of game time per in-game step; each took at most 12
  ticks here). Mining now waits for the server to show the player
  standing, then presses the key once and waits for the mined count. The
  press carries its aim, so one press mines: a run that pressed again
  whenever the last press was answered printed three presses, but the
  server pose already showed the player falling into the mined hole after
  the first; the count only arrives a snapshot later. No first-press bug.
  The old test sent six presses on fixed frame counts without checking the
  player stood, so under load it could press before landing; that cause is
  inferred (the gate's log had no press detail; a failure now prints chat).
- **cpu_time**: the server reads it every tick, so a failed OS read counts
  as zero instead of panicking.

## Evidence (this container, 4 cores, lavapipe)

- `bot_think_time_16` (debug): 4331 us/tick alone; 4623 us/tick with 12
  busy-loop processes, while the whole step went from 5483 to 19085 us/tick.
- `stresslab_flow` (one press): 3 runs alone and 2 under 12 busy loops pass.
- Synthetic variants pass: `stresslab_flow`, `app_item_render` (3),
  `held_items_render`, `item_ghost` (2), `app_flow`, `player_types_render`,
  `sit_first_person`, `view_jitter`; `hardening_packages` 27/27.
- `cargo clippy --workspace --tests -- -D warnings` clean.

Not run here (need v20 content): `night_qa` and the `::content` variants.
The Windows `GetThreadTimes` path is the code `hardening_packages` already
ran on Max's PC, moved unchanged. The PC gate is the proof.
