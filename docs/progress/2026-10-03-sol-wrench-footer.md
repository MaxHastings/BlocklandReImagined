# 2026-10-03 Sol High wrench footer regression

The full candidate gate at `67997ca4` failed two synthetic `field_flow`
cases repeatedly: Rendering and its Copy lock were covered by the detection
region toggle in the ordinary and vehicle wrench dialogs. This was a real
layout defect. The fixed bottom-distance heuristic moved the fallback's last
editable row into the toggle. Read-only inspection of the native layouts also
found a five-pixel overlap with the vehicle Respawn button, despite passing
the existing center-click tests.

`region_view` now inserts its 34-pixel row before the earliest actual footer
action, identified by the existing Send, Cancel, Events and Respawn command
identities. Editable rows retain their authored positions; footer controls
and overlapping footer blockers move down together. The same rule applies to
all three variants, both fallback and native layouts. Generated content and
the field-flow hit tests are unchanged.

New rectangle regressions failed before the fix on fallback Rendering and
native vehicle Respawn. They now pass for all three variants, expanded and
collapsed, at 640×480, 1024×768, 1920×1080 and 960×540 logical sizes. They
verify that the toggle covers no original editable/action control, editable
row positions are unchanged, and rows remain within vertical window bounds.
Native Copy boxes retain their authored oversized-width clipping.

Verification:

- `cargo test -p bri-ui --lib region_row_preserves -- --include-ignored --nocapture`: 2 passed, 0.06s. Before/after logs: `/tmp/bri-v021-sol-wrench-row-before.log`, `/tmp/bri-v021-sol-wrench-row-after.log`.
- `cargo test -p bri-ui --test field_flow -- --include-ignored --nocapture`: all 6 passed, 0.47s; fallback and actual generated content. Log: `/tmp/bri-v021-sol-wrench-field-flow.log`.
- `BRI_CONTENT=/Users/maxhastings/Documents/BlockReImagined/content cargo test -p bri-ui --lib authored_wrench_offscreen -- --include-ignored --nocapture`: passed, 1.39s, no missing textures. Log: `/tmp/bri-v021-sol-wrench-offscreen-after.log`.
- `cargo test -p bri-ui --lib`: 181 passed, 7 generated-content tests ignored, 0.64s. Log: `/tmp/bri-v021-sol-wrench-ui-lib.log`.
- `cargo clippy -p bri-ui --all-targets -- -D warnings`: passed. Log: `/tmp/bri-v021-sol-wrench-clippy.log`.
- Touched-path `git diff --check`: passed.

The native offscreen fixture rendered all three wrench variants at
640×480 and 1024×768/1920×1080 requested scales 1×/2×. Representative
1024×768 requested-2× captures (effective 1.6×, per the UI preference clamp)
were inspected in `artifacts/ui-native-wrench/`: Normal, Sound and
VehicleSpawn. Rendering, region toggle, Respawn and footer controls are
visually separated. Artifacts remain ignored; no original assets are added.
No visible game or operating-system input was used. Root must rerun the
combined candidate gate and Windows checks before release; this local fix
does not close the overall release or interactive acceptance.

Final bounded UI polish after root's capture review: the expanded region panel
now has only two short hints, identifying world units/brick centering and the
build-tool preview. Removed the color legend, event instructions and redundant
Send/Cancel prose. Controls, actions and layout tests are unchanged. Repeated
all six field-flow cases (pass, 0.48s), native offscreen captures (pass, 2.20s,
no missing textures), and UI all-target clippy (pass). Logs:
`/tmp/bri-v021-sol-wrench-hints-{field-flow,offscreen,clippy}.log`.
Inspected the refreshed native VehicleSpawn 1024×768 requested-2× capture;
two hints and clear controls remain. No visible game/input or commit/push.
