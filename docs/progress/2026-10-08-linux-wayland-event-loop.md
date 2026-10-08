# 2026-10-08 Linux: Wayland event loop failure relaunches on X11

A CachyOS player (RTX 3070, NVIDIA 615.71.09) could not run v0.2.5: the GPU
opened on Vulkan and the first frame drew, then `run_app` returned
"Exit Failure: 1".

## Cause

winit 0.30 returns `EventLoopError::ExitFailure(1)` when dispatching its
Wayland connection fails (a compositor or driver protocol error closes the
connection) and logs the reason only through `tracing`, which the client did
not collect, so the log has no reason. winit never falls back to X11 once it
has chosen Wayland. The "Fell back to Vulkan after Backends(METAL | DX12 |
BROWSER_WEBGPU)" warning in that log is harmless and already gone on main
(2026-10-07 backend order).

## Changes

- `crates/client/src/winit_log.rs`: a minimal `tracing-core` subscriber
  forwards winit's warnings and errors to the session log, so the next
  report names the protocol error.
- `system_libraries::relaunch_on_x11`: when the event loop ends with
  `ExitFailure` in a Wayland session and `DISPLAY` is set (XWayland), the
  game starts itself again without `WAYLAND_DISPLAY`/`WAYLAND_SOCKET` and
  exits with that run's code. `BRI_X11_RELAUNCH` stops a second relaunch.

Not reproduced: no NVIDIA Wayland session here. Checked with
`cargo clippy -p bri-client --all-targets -- -D warnings`.

Manual workaround for v0.2.5/v0.2.6: `WAYLAND_DISPLAY= ./launch.sh`.
