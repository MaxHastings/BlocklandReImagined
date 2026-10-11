# Test tiers

A full gate run takes about twenty minutes on Maxwell's PC (ten of them
tests), and running it after every small edit is how forty-minute loops
happened. Pick the cheapest tier that covers what changed, and run the
full gate once per integrated candidate. A tier decides which work to
skip because you did not touch it; it never removes a test from the gate.

`python tools/check.py <tier>` works out the changed crates (files that
differ from the merge base with `origin/main`, plus uncommitted and new
files; `--base REV` or `--crates a,b` override), and prints the time of its
slowest steps. `python tools/check.py plan <tier>` prints what it would run.

| Tier | Command | Runs | When |
|---|---|---|---|
| 1 quick | `python tools/check.py quick` | `cargo fmt --check` and the unit tests (`--lib`) of each changed crate | after every edit; seconds to two minutes on a warm `target/` |
| 2 focused | `python tools/check.py focused` | quick, then clippy `-D warnings` on each changed crate, and every integration test that needs no window or GPU for the changed crates and the crates that depend on them | before committing or opening a PR; a few minutes |
| 3 full | `python tools/check.py full` (same as `python tools/gate.py`; `--push` pushes) | the push gate: build, clippy, `bri-client --check`, every test with `--include-ignored` | once per integrated candidate; the real authority. PRs also run Windows CI |
| soak | `python tools/check.py soak --crates bri-net` | tests named `soak_*`, `endurance_*` or `stress_*` in the crates named, in release | when you want the long cases without the gate |

Notes:

- `--depth 1` limits tier 2 to direct dependents when the changed crate is
  deep in the graph (`bri-world` is under almost everything).
- `bri-client` and `bri-render` integration tests open an offscreen
  renderer or the whole app. Tier 2 runs only their unit tests; ask for the
  render or app test you need with `--tests bri-client:large_build_perf`.
  The gate still runs all of them.
- Each cargo call names one crate, so the Windows command-line limit never
  applies (same reason as `tools/check_format.py`).
- The gate's `tools/gate-known-failures.toml` is unchanged. Nothing in a
  tier retries, skips or hides a failing test, and a new test is not added
  to that file to get green.

## Labelling long cases

A test that runs for more than a few seconds says so in its name, so the
tiers and a reader can find it:

- `stress_` scales a load up (`stress_large_builds_...`, sizes set by
  `BRI_STRESS_BRICKS`).
- `endurance_` repeats an operation many times and checks nothing leaked or
  was lost (`endurance_map_changes_...`).
- `soak_` runs a live system for a long time.

They stay in the gate (it runs ignored tests too). A case too slow even for
the gate, such as a million bricks, takes its size from an environment
variable and defaults to a size the gate can afford; the larger run is
scheduled with `python tools/check.py soak` and recorded in
`docs/progress/`. `#[ignore = "reason"]` is for tests that need hardware or
a person (a real router, a GPU series), with the reason in the attribute.
