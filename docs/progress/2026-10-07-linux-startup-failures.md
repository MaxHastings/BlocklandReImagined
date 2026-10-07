# 2026-10-07 Linux startup failures say what to do

A Reddit player said v0.2.5 "won't boot" on their Linux install, with no
distro or error. The v0.2.5 Linux zip was downloaded into an empty folder and
started in a cloud container (Ubuntu 24.04, Xvfb and headless Weston, Mesa
lavapipe).

## What was checked

- The zip records the executable bit on `bri-client`, `bri-server`,
  `bri-import-addon` and `launch.sh` (`zipinfo`); `unzip` keeps it.
- `objdump -T bri-client` needs up to `GLIBC_2.35` (built on the
  `ubuntu-22.04` runner, `.github/workflows/linux-release-asset.yml`). The
  only linked libraries are libc, libm, libgcc_s, libudev and libasound;
  everything else is loaded at run time.
- With lavapipe it starts and draws on X11 (Xvfb) and Wayland (headless
  Weston), with or without arguments, from any working directory.
- Without `libxkbcommon-x11.so.0` (an X11 session) winit panicked inside
  `EventLoop::new` ("Library libxkbcommon-x11.so could not be loaded").
- Without a Vulkan driver the game stopped with "No usable GPU backend:
  Backends(METAL | DX12 | BROWSER_WEBGPU): ..." and no hint. The build has no
  GL backend, so Linux needs Vulkan.
- On Linux every startup error went only to stderr, and `launch.sh`
  redirected stderr into a log file, so a player saw at most "Client exited
  with code 1", and nothing when double-clicking.

## Changes

- `crates/client/src/system_libraries.rs`: before loading, the Linux client
  opens the libraries winit and wgpu load at run time for the session's
  window system (X11 or Wayland, chosen the way winit chooses) plus the
  Vulkan loader, and names any missing ones with their Ubuntu/Debian, Fedora
  and Arch packages.
- `platform.rs`: the backend order keeps only backends this build has for
  this platform (`wgpu::Instance::enabled_backend_features`), so Linux opens
  Vulkan first instead of failing DX12/Metal first. That also lets the early
  GPU thread work on Linux (GPU opened 2 ms after the window instead of
  55 to 110 ms). When no backend opens, the error starts with what to
  install.
- `bri-crash` dialogs on Linux use zenity or kdialog when a desktop session
  exists, offering to open the logs folder like the Windows message box.
- `launch.sh` shows the game's output in the terminal as well as the logs,
  restores a dropped executable bit, and turns a glibc loader failure into
  "needs GLIBC 2.35 or newer, and this system has glibc X".
- README and TESTER-GUIDE say Linux needs glibc 2.35+ and a Vulkan driver.

## Evidence

The patch applied to the v0.2.5 tag and run against the v0.2.5 zip's content:
X11 and Wayland start and draw (60 s each); `VK_ICD_FILENAMES=/nonexistent`
gives "No working Vulkan driver was found. Install the Vulkan driver ...";
with `libxkbcommon-x11-0` removed it gives "Some system libraries the game
needs are not installed: libxkbcommon-x11.so.0 ... sudo apt install
libxkbcommon-x11-0". `launch.sh` was run against stub clients for the glibc
message, the dropped executable bit and the tee'd logs.
`cargo clippy -p bri-client -p bri-crash --tests -- -D warnings` and the new
unit tests pass.

## Next

The glibc floor stays at 2.35 (Debian 11, Ubuntu 20.04 and RHEL 9 family are
below it). Lowering it means building in an older container or with a glibc
target pin; decide once a player on such a system reports.
