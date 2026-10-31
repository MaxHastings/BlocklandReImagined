# Display modes and common window sizes

Audited issue #416 against the current client. Fullscreen already calls winit's
`Fullscreen::Borderless` on the current monitor and uses its native physical
size; adding a second, identically implemented borderless mode would not fix a
demonstrated defect. Windowed choices, however, came only from the monitor's
exclusive video modes. A window can have a valid client size that is absent
from that list, including the reported 1280×1024 on a sufficiently large
desktop.

The platform now merges its monitor-reported modes with a shared, curated set
of common 4:3, 5:4, 16:9, 16:10, 3:2 and ultrawide window sizes, filters them
against the desktop and decorations, sorts and deduplicates. Monitor-reported
nonstandard sizes remain available. The options screen uses the same common set
as its pre-monitor fallback. Fullscreen behavior, the GPU-size check and the
existing change/revert flow stay in place. A separate aspect-ratio dropdown is
not added: the existing resolution popup supports scrolling, and an extra
control is not needed for the reported gap.

The new platform regression failed before the change and now passes for SXGA,
QXGA on a sufficiently large desktop, 3:2, 16:10 and ultrawide choices absent
from the exclusive-mode list. Existing window-boundary/deduplication coverage
now also checks a nonstandard monitor mode. The options test selects and
applies 1280×1024 as a windowed request; the existing fullscreen-toggle test
still passes. Commands: `cargo test --locked --profile playtest -p bri-client
--lib windowed_sizes`, the same with
`common_window_sizes_do_not_depend_on_exclusive_monitor_modes`, `cargo test
--locked --profile playtest -p bri-ui --lib resolution` and the same with
`a_five_by_four_window_size_remains_selectable`. Warning-free changed-crate
Clippy: `cargo clippy --locked --profile playtest -p bri-client -p bri-ui
--lib -- -D warnings`. The options-screen unit target passed 46 cases, with
four existing ignored cases not run.

These are headless list/dispatch checks. Platform CI and actual window-manager,
monitor/DPI and multi-monitor behavior remain release validation; no visible
window or interactive playtest was run. The adjacent automatic draw-distance
issue #417 remains a separate investigation.
