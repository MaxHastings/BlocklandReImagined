# 2026-10-02 Add-Ons: one-click check boxes, no lag

Max (v0.1.13 ask): turning Add-Ons on and off took a double-click per row,
and the tick showed up late.

Cause of the lag: every toggle ran `App::apply_packages` on the UI thread,
reloading every Add-On's bricks, weapons, items, vehicles, icons and health
checks, and the screen waited for that answer before ticking the box.

Changes:
- `bri_ui::view`: text lists take a `checkColumn`; cells written with
  `check_cell` draw the check box bitmap (`GuiCheckBoxProfile`), and one
  click on the box sends `EventKind::Toggle` (never half of a double-click).
- Add-Ons list: box, warning mark, name. A click on the box toggles; a
  click on the name shows details. The row ticks at once and reverts if
  the host refuses. Double-click and the Enabled box still work.
- Host: `SetAddOnEnabled` / `DefaultAddOns` only write the lists
  (`add_ons_listed`). New `UiAction::ApplyAddOns`, sent when the screen
  closes, loads the list once; hosting still loads it if it was skipped.

Evidence: `cargo test -p bri-ui --lib addons` (11 passed, new
`one_click_on_a_box_ticks_it_at_once`); `cargo clippy -p bri-ui -p bri-client
--tests -D warnings` clean. New ignored host guard
`default_add_ons::turning_an_add_on_off_loads_nothing_until_the_screen_closes`
needs generated content; the Gate runs it.

Next: closing the screen still loads on the frame it closes (once, not per
click). Loading off the UI thread needs `ClientContent` to be `Send` (its
`ui_pack` is an `Rc`), which belongs with the app.rs split.
