# 2026-11-25 No X11 relaunch after a mid-session display loss

The game closed by itself and came back fullscreen on Linux (CachyOS, KDE
Wayland, NVIDIA RTX 5090 laptop), most often while the player was in the
start-game Add-On selection. The state logs showed it: `settings.json` stores
1920x1200 fullscreen, so any process restart returns to that mode; and
`target/release/logs/` held pairs of session files starting 16-72 seconds
apart with no crash reports between them (for example
session-20261126-052101.log then -052137.log). A clean exit plus an immediate
second process is the signature of `system_libraries::relaunch_on_x11`, not a
crash.

The chain: winit 0.30's Linux backends end the event loop with
`EventLoopError::ExitFailure(code)` when the display connection fails — X11 on
a failed poll, Wayland on a `flush()` or dispatch error (wayland-backend treats
any non-EAGAIN flush error as permanently fatal). `main.rs` then re-executed
the whole game process on X11 whenever that error appeared, no matter how long
the session had been running. The mechanism was added for a startup failure
(NVIDIA on CachyOS failing right after the first frame), but it fired mid-
session too and discarded the running game — menus, world, everything — which
is what the player saw as "it closes and starts back up in fullscreen".

The fix bounds the fallback. `platform::run` now returns loop failures wrapped
in a new `LoopFailure { source, ran_for }`, where `ran_for` is the time from
entering the event loop to the failure; its `Display` names a lost display
connection for the player and keeps winit's detail in the chain. A failure
within `STARTUP_FAILURE_LIMIT` (5 seconds) still counts as startup, so the X11
fallback keeps working for the original case. After that,
`relaunch_on_x11` declines: the existing "could not continue" alert shows what
happened ("the connection to the display server was lost after Ns") and the
player restarts deliberately instead of the process re-executing under them.

Checks (branch `bionic/display-loss-no-relaunch`, off main eeaa41735):
`python tools/check.py focused --crates bri-client` — fmt clean, clippy
`-D warnings` clean, unit tests 814 passed with 5 failures that are all
`wgpu error: Out of Memory` (or a SIGSEGV after the suite) in offscreen render
tests; each failed test passes when run alone
(`cargo test -p bri-client --lib <name> -- --test-threads=1`), and which one
fails varies between runs — VRAM contention on this laptop, not the change.
New tests: `a_loop_failure_is_startup_only_while_the_window_cannot_have_gone`,
`a_lost_display_connection_is_named_for_the_player`,
`a_startup_failure_keeps_its_plain_wording` (platform) and
`a_mid_session_connection_loss_is_never_relaunched` (system_libraries).

Next: Maxwell playtests the rebuilt binary on his machine. If a display loss
still happens, it now ends in a dialog naming the failure instead of a silent
re-exec; that text is the evidence for whatever comes next. The deeper question
— why the Wayland connection drops intermittently (driver/compositor side) —
stays open and is not addressed by this change.
