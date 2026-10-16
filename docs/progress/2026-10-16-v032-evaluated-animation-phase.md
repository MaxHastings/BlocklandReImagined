# 2026-10-16 v0.3.2 evaluated active animation phase (D2a)

Root granted D2a only from immutable docs checkpoint
`efc21d370ea6f47c168a1a551869fd59bb6fa3f1`, following accepted D1
`63edd2300eaec1a9f8a63ad9073ab6263d45eb96`. Changes stay on the isolated
`codex/v032-engine-seams` branch, outside v0.3.1. Production reservation was
`sim/src/session/animation.rs`, `session.rs` exports only, and the new focused
`sim/tests/evaluated_animation_phase.rs`, plus own architecture/progress docs.

Added `Session::evaluated_animation_view()` returning Session's current tick,
existing mutation revision and evaluated active entries. Each entry retains the
full unchanged ActiveAnimation anchor/request/binding/clip snapshot and names
the separate `evaluated_phase`. That phase calls the existing private `phase_at`
directly. It remains native clock seconds before sampling loop/clamp. The query
uses `&self`, owns its returned data, and does not advance clocks, change anchors,
drain records or alter admission/identities/terminal outcomes. Time can advance
without a revision change; the type documents that revision alone cannot cache
evaluated phases. Existing `animation_view` retains its anchor semantics.

No new serde/wire, held-final entries, callback, producer, package hook/counter,
motor/client extraction, gameplay origin or extra production paths. Natural and
zero-duration completion still remove active entries. D2b's applicability and
bounded capacity review remains separate, as do original Halo/AA source laws.

Before production edits, added four actual Session controls covering anchor vs
tick/revision, duplicate identity, pause/resume/rate, loop/unwrapped terminal
phase, zero-duration replacement, Natural removal and disconnect/resume life
invalidation. The unchanged clock passed all four and its existing 15 controls:
`cargo test -p bri-sim --test evaluated_animation_phase --test authoritative_animation_completion`
with process-local CARGO_BUILD_JOBS=2 and RUST_TEST_THREADS=8, CARGO_TARGET_DIR
unset (`target/d2a-baseline-corrected.log`). Initial fixture compilation used
unsupported `f64::from(u64)` for the tick constant; corrected to the same explicit
numeric cast before any production change (`target/d2a-baseline.log`). No runtime
failure was waived.

Five additional actual Session controls exercise evaluated tick/rate/pause,
unwrapped loops, unchanged anchor/request/identity, duplicate admission, no held
entry after Natural/zero, repeated queries and exact remaining terminal order,
prepared-ticket validity across queries/failed admission, same-tick replacement,
disconnect/resume and independently owned old snapshots. The post-change targeted
command above passed all 15 existing clock tests and all nine controls in the
new file (`target/d2a-targeted.log`).

`python tools/check.py quick --crates bri-sim` passed formatting and 403 sim unit
tests, zero failed, four existing ignored, in 200 seconds (`target/d2a-quick.log`).
`python tools/check.py focused --crates bri-sim` passed all eight steps in
2620 seconds with the normal dependent closure, including sim all-target Clippy
with `-D warnings` (`target/d2a-focused.log`). Across 202 test summaries:

| Crate | Passed | Failed | Existing ignored |
| --- | ---: | ---: | ---: |
| bri-addon-import | 229 | 0 | 3 |
| bri-chaos | 272 | 0 | 26 |
| bri-client (unit) | 590 | 0 | 76 |
| bri-net | 217 | 0 | 20 |
| bri-sim | 1078 | 0 | 126 |
| bri-stresslab | 3 | 0 | 0 |
| Total | 2389 | 0 | 251 |

Sim includes all nine evaluated-phase controls and the 15 existing clock tests;
net includes the D1 metadata/host controls and its crossover soak. The standard
tier's existing ignores remain unchanged. No retry or tier exclusion was added.
Dependency-feature compilation and the existing importer/chaos suites account
for most elapsed time. All Cargo checks used the same two-job/eight-thread test
settings with CARGO_TARGET_DIR unset. No builds were interrupted.

`git diff --check` passed. A read-only inline summary command had a shell quoting
error after the successful focused run; a literal Python here-string corrected
it and extracted the totals above from the unchanged log. This did not change
or rerun any test. `git diff --cached --check` also passed before committing.

Next work requires root review of a clean immutable D2a checkpoint before any
further grant. No full integrated gate, visible game session or original-asset
consumer claim belongs to this bounded query change.
