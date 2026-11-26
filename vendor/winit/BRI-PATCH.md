# Local winit patches (v0.30.13)

This is the published Apache-2.0 winit 0.30.13 source, retained with its license.
Original source: https://github.com/rust-windowing/winit/tree/v0.30.13

Cargo.toml uses a local crates.io patch, excluding the third-party library from
the first-party workspace. No dependency versions are upgraded by this backport.
Future upstream upgrades must retain both corrections below or verify an
equivalent upstream fix before removing them.

## X11 legacy wheel-button mapping

The only other runtime patch is the X11 legacy wheel-button mapping in
src/platform_impl/linux/x11/event_processor.rs and its small scroll_button.rs
helper. XInput2 dispatch supplies both XI_ButtonPress and XI_ButtonRelease;
upstream maps both to one wheel notch for non-emulated buttons4 through7.
Suppress the release for these wheel buttons only. Existing emulated-pointer
suppression, smooth axis motion, ordinary mouse buttons and every other platform
remain unchanged. No time-based filtering or scroll-rate limit is introduced.

The helper is included directly by crates/client/tests/x11_wheel.rs so its exact
production mapping and the real inventory/paint UI can be tested headlessly on
all build platforms. Those tests do not simulate an X server or confirm the
reporter's particular Linux backend/device.

## Wayland transient flush fix

The published event loop ends the whole game when `connection.flush()` fails,
regardless of why (src/platform_impl/linux/wayland/event_loop/mod.rs). On Linux
with wayland-backend's system libwayland client (`client_system`), a full kernel
send buffer makes `wl_display_flush` fail with EAGAIN while the compositor is
momentarily not reading; wayland-backend correctly treats that as transient — it
does not store it and keeps the unsent data for the next flush. Upstream still
exited the loop, so a running game closed by itself (most often while sitting in
menus on KDE Plasma Wayland) with no error anywhere: EAGAIN is never logged by
design, and winit's bare `Exit Failure: 1` named nothing.

The patch keeps the exit for permanent flush errors — now logged through
tracing first, which bri-client forwards to the console log and this run's
session log (crates/client/src/winit_log.rs) — and skips the fatal handling for
EAGAIN so libwayland retries the data on the next iteration. X11 and every other
platform are unchanged.
