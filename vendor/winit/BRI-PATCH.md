# Winit 0.30.13 X11 legacy wheel backport

This is the published Apache-2.0 winit 0.30.13 source, retained with its license.
Original source: https://github.com/rust-windowing/winit/tree/v0.30.13

The only runtime patch is the X11 legacy wheel-button mapping in
src/platform_impl/linux/x11/event_processor.rs and its small scroll_button.rs
helper. XInput2 dispatch supplies both XI_ButtonPress and XI_ButtonRelease;
upstream maps both to one wheel notch for non-emulated buttons4 through7.
Suppress the release for these wheel buttons only. Existing emulated-pointer
suppression, smooth axis motion, ordinary mouse buttons and every other platform
remain unchanged. No time-based filtering or scroll-rate limit is introduced.

The helper is included directly by crates/client/tests/x11_wheel.rs so its exact
production mapping and the real inventory/paint UI can be tested headlessly on
all build platforms. Those tests do not simulate an X server or confirm the
reporter's particular Linux backend/device. Future upstream upgrades must retain
the correction or verify an equivalent upstream fix before removing this patch.

Cargo.toml uses a local crates.io patch, excluding the third-party library from
the first-party workspace. No dependency versions are upgraded by this backport.
