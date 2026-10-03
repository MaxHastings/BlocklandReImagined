# 2026-10-02 GUI style review

Reviewed representative frames from the existing 90-frame UI probe and the
native wrench captures against recovered v20 profiles/layouts and UI research.
The review stays within v20's visual vocabulary: stock bitmap buttons, native
window/tab/section styling, compact labels and short hints. The concrete
outliers were the flat Options Apply control and the 22 px Escape Menu Rule
Workshop action, which touched its neighbors. Apply now uses the stock bitmap
button at 30 px. Rule Workshop is 38 px high with 6 px gaps above and below;
Player List and later actions move down 26 px, and the centered window grows
to 447 px to fit the 480 px logical canvas. The options and Workshop
authored-render fixtures accept `BRI_CONTENT`, matching the wrench fixture.

The existing dirty wrench changes were preserved. The rendered 185×30
Detection region action has native bitmap art; its comparison popup leaves
room for the X remove control. Workshop's 640×480 and 853×480 frames show its
full two-column choices, and its 400×300 frame keeps the list scrollable in a
single column. The Options frame shows Apply using the rounded stock artwork.
Extended the authored actual-pack offscreen fixture to Admin, Environment
Simple/Advanced, Explain, Add-Ons with a selected package, Pause, and
Mini-Game settings/teams. Those additional states are captured at 640×480;
the Admin fixture has no active authority, so its disabled state is not an
active-admin appearance review.

Commands and evidence:

- `BRI_CONTENT=/Users/maxhastings/Documents/BlockReImagined/content cargo test -p bri-ui --lib screens::options::tests -- --nocapture` — 31 passed, 0 failed, 1 ignored after the final source edits.
- `cargo test -p bri-ui --lib screens::menus::tests::escape_workshop_row_has_native_gaps_and_window_fits_canvas -- --exact` — passed; verifies both 6 px gaps, the 38 px row, and centered-window fit at 640×480.
- `BRI_CONTENT=/Users/maxhastings/Documents/BlockReImagined/content cargo test -p bri-ui --lib screens::options::tests::authored_options_save_players_offscreen -- --ignored --exact` — passed after the override. Output: `artifacts/ui-native-dialogs/`.
- `BRI_CONTENT=/Users/maxhastings/Documents/BlockReImagined/content cargo test -p bri-ui --lib screens::workshop::tests::workshop_offscreen -- --ignored --exact` — passed after expanding capture coverage. Output: `artifacts/workshop-ui/`.
- `BRI_CONTENT=/Users/maxhastings/Documents/BlockReImagined/content cargo test -p bri-ui --lib screens::wrench::tests::authored_wrench_offscreen -- --ignored --exact` — passed. Output: `artifacts/ui-native-wrench/`.
- `cargo build -p bri-ui --bin ui_runtime_probe` — passed.
- Ran the rebuilt probe from the main checkout with the read-only main content pack and output directed to this worktree's `artifacts/ui-style-audit-final/`. It rendered 90 frames at 1024×768, 1920×1080 1× and 2×; host/cancel flows and external UV checks passed. No visible game window or OS input was used.

One initial capture attempt pointed at a nonexistent `BlocklandReImagined`
checkout path. The actual main checkout is `BlockReImagined`; the corrected
commands read `/Users/maxhastings/Documents/BlockReImagined/content` and do
not modify it. `docs/audits/gui-style-review.md` records the broader review
scope and the boundary between offscreen evidence and Maxwell's interactive
acceptance. Maxwell's human review remains follow-up work.
