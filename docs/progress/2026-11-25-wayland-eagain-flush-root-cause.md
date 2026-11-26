# 2026-11-25 Root cause: winit treats a transient Wayland flush EAGAIN as fatal

Phase 3 of the "closes and restarts fullscreen" bug. Phase 1 stopped the silent
re-exec; phase 2 (commit `c793e52`) made sure the reason for a display loss is
captured in the session log. The capture build died twice on the test laptop —
after **702 s** and after **140 s**, both "Exit Failure: 1" — and both session
logs were header-only. That silence turned out to be the answer.

## Why a silent death is diagnostic

With `client_system` enabled (winit's wayland-backend feature), the active
client implementation at runtime is the **sys** one (C libwayland via dlopen) —
not the pure-Rust `rs` implementation that earlier analysis read. In the sys
implementation, every stored fatal error is logged through the `log` crate
(`store_and_return_error` → `log_error!`, which our phase-2 bridge forwards to
the console and session log; libwayland's C protocol errors arrive through the
trampoline installed under the same feature). The one exception: a flush that
fails with **WouldBlock/EAGAIN** is returned *without being stored and without
being logged* — by design, because it is transient.

So "header-only session log + Exit Failure: 1" means the fatal flush error was
EAGAIN. The chain: KWin momentarily stops reading (busy frame, GPU stall); the
kernel send buffer fills; `wl_display_flush` returns -1/EAGAIN; wayland-backend
correctly keeps the unsent data for the next flush — but winit 0.30's Wayland
event loop does `if self.connection.flush().is_err() { set_exit_code(1); return; }`,
killing a running game over a momentary compositor stall with no message
anywhere (EAGAIN is never logged by design, and the exit code is hardcoded 1).

This explains everything observed: intermittent timing per session (14 s to
702 s), both laptops regardless of GPU (client-side logic, not hardware — the
NVIDIA-driver hypothesis is unnecessary), no reason ever recorded since the
original October incident, and prominence in menus simply because that is where
the player sits.

## The fix (branch bionic/display-loss-no-relaunch)

- `vendor/winit/src/platform_impl/linux/wayland/event_loop/mod.rs`: a flush
  failure with `ErrorKind::WouldBlock` no longer ends the loop — libwayland
  retries the data on the next iteration, exactly like any blocking client.
  Permanent flush errors still exit, but are logged through tracing first (new
  helper `is_transient_flush_error`). Documented in `vendor/winit/BRI-PATCH.md`.
- `crates/client/src/winit_log.rs`: the tracing half now routes through the same
  report function as the `log` facade half, so winit's own error messages also
  reach this run's session log file (previously only stderr). New test:
  `a_fatal_display_line_reaches_the_console_log`.

## Checks

`python3 tools/check.py focused --crates bri-client,bri-crash`: fmt clean,
clippy `-D warnings` clean. Unit tests pass in a contention-free run; under GPU
load 5-6 offscreen-render tests flake (`wgpu error: Out of Memory`), each
passing alone with `--test-threads=1` — the known VRAM-contention pattern on
this laptop, unrelated to the change.

## Next (needs Maxwell's playtest)

Run the rebuilt binary on the test laptop and sit in the Add-On selection as
before. Expected: no more self-closing at all. If a death still happens, it now
names itself in `target/release/logs/session-*.log` — a permanent flush error,
a dispatch failure, or a protocol error (interface + opcode) — and that line is
the next evidence.
