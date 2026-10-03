# 2026-10-03 Popup word-search regression expectations

Windows CI for the older `abcb22e1` candidate exposed two stale popup-search
expectations in `crates/ui/tests/view_input.rs`. The recorded log is
`/tmp/bri-v022-windows-candidate-failure.log` at lines4695 onward: a query for
`vehicle s` returned NONE, Vehicle Smoke and Vehicle Bubbles; `emitter 1999`
returned twelve matches rather than ten. No Windows-specific behavior or new
production defect was found in this source review.

The shared helper intentionally ranks exact phrases first, then matches all
query words anywhere in the item/alias. `s` also occurs in Bubbles. Ten emitter
names start with the full phrase (`19990` through `19999`); `01999` and `11999`
match the two words without containing that phrase. Both are valid later-ranked
results under the shipped search semantics.

The tests now assert the complete ranked authored IDs, the exact-prefix best
highlight, ghost completion, narrowing to `vehicle smo`, and End selecting the
last token-only result's actual ID. Existing keyboard/cancel/commit assertions
and the twenty-thousand-item responsiveness bound remain. No test is skipped;
no production search code is changed.

`cargo test -p bri-ui --test view_input` passes all10 tests with no failures or
ignored tests (0.10s test runtime, 7.96s compile). Log:
`/tmp/bri-v022-sol-popup-word-search.log`. Touched-file formatting and
`git diff --check` pass. This Mac run verifies the platform-independent ranking
expectations; Windows CI must rerun on the integrated candidate.
