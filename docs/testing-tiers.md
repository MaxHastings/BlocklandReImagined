# Test tiers

Maxwell replaced the mandatory exhaustive merge gate on 2026-10-28. Ordinary
changes use checks for the affected behavior and passing platform CI. Keep the
extended suites for nightly/manual runs, releases and changes that need them.
A private playtest ships after relevant checks and packaged startup; it does
not wait for merge checks, ZIP creation or release LTO.

| Scope | Command | What runs |
|---|---|---|
| Quick | `python tools/check.py quick` | Formatting and changed crates' unit tests. |
| Focused | `python tools/check.py focused` | Formatting, changed crates' warning-free clippy, tests of changed crates and direct consumers. |
| Commit gate | `python tools/gate.py` | History/protocol/large-blob checks, then tool tests, documentation checks and focused checks on the exact committed tree. |
| Extended | `python tools/gate.py --full` or `python tools/check.py full` | Workspace build/clippy, native content startup, full test targets including ignored cases, affected save corpus and host performance budget. |
| Soak | `python tools/check.py soak --crates bri-net` | Explicit stress/endurance/soak cases, including ignored tests. |

`--push` keeps the guarded rebase/check/push path. The pre-push hook stays
enabled. A scoped pass is never recorded as an extended pass. Release tooling
explicitly requests `--full`; `--release-content` also implies it.

## Select relevant work

Changes are measured from the merge base with `origin/main`, including working
changes. Override with `--base REV` or `--crates a,b`. Use `check.py plan focused`
to inspect the plan. Focused checks include direct consumers by default;
`--depth 0` selects changed crates alone, `--depth -1` includes all consumers.
Cargo groups selected packages to reuse their dependency graph.

Client, renderer, effect renderer, UI and chaos integration suites contain
expensive GPU/whole-app/fuzz cases. Focused runs their unit tests by default.
Name the integration targets relevant to your change explicitly, for example
`python tools/check.py focused --tests bri-client:weapon_debris_render`. Do this for
both the changed behavior and important consumers; a unit-only pass does not
establish integrated gameplay or visual fidelity.

Content, packaging and schema changes still need their corresponding checks,
including `bri-client --check` against the actual content where relevant. Wire,
save, authority and shared physics changes justify the extended gate. Record
commands and scope, failures and remaining acceptance in the progress entry.

## CI and suite ownership

Ordinary PR and main CI use the same bounded `pr` tier. Windows builds all five
shipping executables, checks imports and clippy; PR tests also run on Linux.
Nightly/manual CI runs the extended content-free suites. Generated/private
content checks remain local or release checks because hosted runners lack it.
`--ci-test --tier main` remains available for the intermediate legacy tier.

`tools/gate-known-failures.toml` owns the CI selection and each deferred target's
reason. Broad client/render/network/chaos end-to-end matrices and eight measured
UI/effect GPU matrices run in extended validation. Core unit tests and fast
UI/effect input/state integration tests remain in ordinary CI. Directly affected integration targets must still be
checked for a change even if their general matrix is deferred.

Each shard now scopes Cargo target filters to the selected packages. A shard
requesting one library must not compile every workspace unit-test executable.
Pure CLI probes without tests set `test = false` in their manifests: their real
executables still build and lint, while empty libtest copies are not linked.
Do not disable a harness containing tests or modules with tests.

Retire duplicate or obsolete cases when their meaningful contract is already
covered. Keep distinct failure boundaries. Do not weaken assertions, hide new
failures or add arbitrary skips to turn a run green. Failed named cases may be
retried once and reported as flaky; persistent failures, hangs and crashes fail.

## Long cases and playtesting

`stress_` scales load, `endurance_` repeats operations, and `soak_` runs a live
system for a long time. Keep these explicitly selectable. Large settings and
performance campaigns belong in dedicated runs with measured evidence.

Use the warm worktree and `--profile playtest` for previews. Finish the requested
build before starting expensive validation. Never run local GPU suites, full
gates, soaks or benchmarks while Maxwell plays: contention invalidates his
performance observations. Maxwell owns every interactive playtest.
