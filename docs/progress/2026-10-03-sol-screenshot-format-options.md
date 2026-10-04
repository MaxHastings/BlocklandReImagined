# 2026-10-03 Screenshot format choice

Maxwell requested high-quality JPEG as the default screenshot format, with PNG available. Options → Advanced → Gui Options now exposes a labeled Screenshots menu, JPEG (high quality) / PNG (lossless), using the existing native popup styling. It uses the existing Options draft lifecycle: Done/Escape commits settings; abandoning the screen without committing retains the previous preference.

The shared UI read API is `bri_ui::screens::options::{SCREENSHOT_FORMAT, ScreenshotFormat, screenshot_format}`. `$pref::Screenshot::Format` defaults to JPEG when unset or invalid. `ScreenshotFormat::{Jpeg,Png}.extension()` returns `jpg` / `png`; PNG reads case-insensitively. Root owns the screenshot path, asynchronous JPEG95 encoder and encoder tests. This UI does not expose a misleading quality control or duplicate screenshot encoding.

Focused regressions cover defaults/invalid preferences, labeled choices, draft abandonment, Done persistence/reopen, and the menu's bounds at 400×300 and 1024×768. An ignored original-content test checks those bounds and captures the open menu offscreen without a game window.

Validation:

- `cargo test -p bri-ui --lib screenshot` — 3 passed, 1 native-content test ignored pending the explicit run; `/tmp/bri-v022-sol-screenshot-ui.log`.
- `cargo test -p bri-ui --lib native_screenshot_menu_fit_and_capture -- --ignored` — 1 passed, 0.92 seconds; `/tmp/bri-v022-sol-screenshot-native.log`. The 400×300 and 1024×768 native open-popup captures were visually inspected: labeled choices fit the existing Advanced scroll page and native window. Files: `artifacts/ui-native-screenshot-format/Options-Screenshots-{400x300,1024x768}.png`.
- Root's `cargo test -p bri-client --lib screenshot_jpeg_and_png_use_their_real_formats` — 1 passed; `/tmp/bri-v022-sol-screenshot-encoder.log`.
- Root's guest Save/Load permission check, `cargo test -p bri-ui --lib a_remote_non_admin_can_save_but_cannot_load` — 1 passed; `/tmp/bri-v022-sol-guest-save-ui.log`.
- Scoped UI/client all-target clippy initially stopped in the new sim ledger dependency's type-complexity warning; root factored key/value aliases and the rerun passed (26.68 seconds). `/tmp/bri-v022-sol-music-jpeg-clippy-final.log`; initial failure `/tmp/bri-v022-sol-music-jpeg-clippy.log`.
- Source formatted and `git diff --check` passes. Captures are offscreen; no interactive game window was launched.
