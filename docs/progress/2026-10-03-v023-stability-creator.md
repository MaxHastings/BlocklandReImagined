# 2026-10-03 v0.2.3 stability and creator-state audit

Lane based on main `0eebedb81ad3d3990b0277dfe4b2518ce1a63e2b`, with root owning
shared integration. No shared production source, manifests, schemas, lockfile,
original installations or existing progress notes were edited by this lane.
No commits, pushes, new worktrees, interactive input or visible game launches
were performed. Cargo/GPU verification is root-owned until a compute lease is
available.

## Crash disposition

Read `/Users/maxhastings/Downloads/crash-20261003-031241.txt` and
`client-20261003-225659-874.stderr.log`, the existing stability audit, and the
v0.2.2 effects-atlas progress entry. The original Windows Bedroom firefight
main-thread `f32::clamp` panic has two NaN bounds and no resolved caller or
native frame addresses. The Gun-to-Gravity-Gun edit remains a temporal
observation. Existing audits already trace validated content-bound animation,
vehicle, control, audio and geometry paths and retain runtime physics/dependency
corruption as unexcluded. This lane found no new caller proof, so adds no
speculative NaN clamps and claims no causal cure.

The latest stderr ends in `Invalid effects instance`; root's already reproduced
and fixed CPU-pack/GPU-atlas reload defect remains the relevant independent
repair. This lane does not reimplement it or infer exact attribution from the
field-free original stderr.

## Concrete creator-state defects and proposed correction

`Session::minigame_snapshot` includes a package/key only when its declared
`per_minigame` map contains an entry for the chosen game. Existing
`restore_per_minigame` only visits entries present in the saved snapshot. An
ordinary build saved before Slayer's fly-through path exists therefore has
`packages: {}`. Loading it after creating a path retains that later path;
Slayer's `mg_loaded` reads it, resets its test/countdown state and reuses it.
This is code-path proof, not an executed human reproduction.

A second admission gap checks only each saved inner entry, rather than the
resulting declared map key. Thirty 100-byte strings serialize to a valid
3091-byte entry. A key with that entry for game 2 and `"current"` for game 1 is
3111 bytes. Loading an independently saved 3091-byte entry into game 1 produces
a 6193-byte key, beyond `state::check_value`'s 4096-byte limit. Existing namespace
and server budgets are much larger and admit it. The resulting public state
fails `PackageStateView::validate`; persisted state cannot pass `Store::decode`.
Depth has the same envelope problem: an inner depth-four entry produces a
depth-five key. This is concrete boundary proof using the production limits.

Reviewable patch: `/tmp/bri-v023-stability-creator.patch`. Original and proposed
file mirrors are under `/tmp/bri-v023-stability-creator-original` and
`/tmp/bri-v023-stability-creator-proposed`.

The proposed existing canonical restore path visits running packages' declared
per-game keys and replaces only the selected game's entry. Omitted values
remove that entry; explicit empty text remains data. Other games, ordinary
keys, player state and unknown saved package/keys are untouched. Present values
are admitted against the whole key and existing namespace/server budgets before
mutation. Rejected values retain the existing entry and report through existing
package diagnostics (`state.restore` / `state.budget`). No blanket clear,
schema changes, hidden recovery or permission bypass is added.

Three proposed `script_api` regressions use ordinary MiniGame creation/package
commands, `SaveBuild`, native file encode/decode, `LoadBuild`, and the actual
`on_minigame loaded` hook. They cover absence versus explicit empty data;
neighboring game and ordinary-key preservation; unknown saved namespace/key
isolation; a real merge over the byte limit; depth-envelope rejection;
loaded-hook observations; public-state validation; and persistent-store decoding.
The patch deliberately leaves the less-proven invalid Add-On override/default
case for later audit.

## Verification and remaining work

- Read-only repository/crash/source inspection completed.
- A lightweight JSON-size proof confirmed 3091 / 3111 / 6193 bytes against the
  production 4096-byte value limit.
- `rustfmt --edition 2024 --check` on both proposed files passed.
- `git apply --check /tmp/bri-v023-stability-creator.patch` passed against the
  shared checkout when handed to root.
- Rust compilation, before/after runtime regressions and strict Clippy are
  **pending**. No Cargo command was run without root's compute lease.

Root should apply tests alone and run
`cargo test --locked -p bri-sim --test script_api saved_build -- --nocapture`
to retain negative evidence, apply the correction and rerun, then run the
complete `script_api` target and scoped strict Clippy. Further release gate,
Windows CI/package evidence and Maxwell's interactive acceptance remain open.

## Independent Windows CI failure triage

Root provided `/tmp/bri-v022-windows-ci-failed.log` for Windows check run
`37161683721`. The terminal failure is an actual assertion in
`bri-chaos/bot_physical_objectives`, rather than a job timeout, graphics-device
error or missing fixture. Thirteen of fourteen target tests passed;
`repeated_object_entry_requires_real_exit_and_reentry_for_each_physical_method`
failed its canonical winner assertion in the declared-hold, `echo-chamber`,
offset-24, initially-inside subcase. Root assigned causal ownership to the NPC
lane; this lane only reviews it independently and changes no NPC source.

The recorded trace selects Hold/rearm at tick 123, reports `objective resource
claimed` at 939, and later falls back to Contact/rearm. The object starts at
z=56.25 within the wide region centered at z=57.25, then drifts toward higher z;
its nearest initial exit lies below the region's z=47.25 lower bound. This
suggests inspecting exact native grip target, rearm geometry and useful-progress
lease accounting before changing any deadline or fixture. It does not alone
establish a cause. The owning lane was sent these observations, including the
live-grip/advisory-lease distinction and the initial trace's carried author.
Release-tag/source identity stays unchanged; no known-failure exemption or
coverage relaxation was proposed.

## First root execution and fixture correction

Root's first tests-only baseline run is
`/tmp/bri-v023-saved-state-before.log`. The pre-existing saved mini-game test
passed; all three new tests stopped at `LoadBuild` admission with
`Build contains no bricks`. These results **do not reproduce the intended
restore defects**. The new fixtures were missing ordinary brick data.

Corrected test-only diff:
`/tmp/bri-v023-stability-creator-fixture.patch`, applicable to root's already
applied test hunk. The full proposed patch was refreshed too. Existing
`Game::new` behavior stays equivalent through a definition-parameterized
constructor; only the three new fixtures declare a synthetic 1x1 plate. Before
MiniGame creation, their helper loads one authored plate at
`[20.25, 0.1, 20.25]` through the same typed `LoadBuild` path, asserts that the
real brick was placed, and subsequently snapshots it naturally through
`SaveBuild` and native encoding. Acceptance assertions are unchanged. Formatting
and incremental patch applicability passed. Root still needs the corrected
before/after run; no Cargo command was performed by this lane.

## Root after-fix verification

Root applied the corrected fixtures and production restore correction, then ran
the full `script_api` target. `/tmp/bri-v023-saved-state-after.log` reports
**24 passed, 0 failed, 0 ignored, 0 filtered out**, 0.74 s. This includes all
three added saved-build boundary regressions and the existing saved mini-game
behavior. This lane read the log; root owns command execution. The earlier
empty-brick baseline remains recorded above and must not count as defect
reproduction. Release/Windows and human creator acceptance remain open.
