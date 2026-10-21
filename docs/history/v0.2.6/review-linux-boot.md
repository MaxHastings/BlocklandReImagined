> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-linux-boot.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: Linux startup failures (9ffe6d2 on 498d7f58, claude/fix-linux-boot-c5rao2)

Verdict: **passes with one change to the preflight (a). Merge that sha.** Everything else is right.

## (a) Preflight false positives: one real case, one likely

- **What's sound:**
  - It refuses only when `dlopen` of an exact SONAME fails.
  - The SONAMEs are the standard ones on every distro (`libX11.so.6`, `libvulkan.so.1`, ...).
  - `libloading::Library::new` uses the same dlopen and search path that winit's x11-dl, xkbcommon-dl and wayland-sys use, so Flatpak and the Steam runtime answer it exactly as they answer winit.
  - It runs only in `run()`, not in `--check` or the server.
  - Vulkan is fine to require, since this build has no GL backend.
- **Real false positive: a Wayland session that would fall back to X11.** If `WAYLAND_DISPLAY` is set, the check demands the Wayland libraries. winit 0.30 tries Wayland and **falls back to X11** when it can't connect (inferred from winit's event loop; not re-read here). So a box with XWayland and X11 libraries but no `libwayland-client` would run today and be refused now.
  - Fix: on a Wayland session, pass if **either** window system's set loads. Only on X11 demand the X11 set. Shared xkbcommon and Vulkan stay required either way.
- **Likely false positive: `libwayland-cursor.so.0`.** `wayland-cursor` 0.31.14 (the version in Cargo.lock) reads cursor themes in pure Rust through `xcursor`. As far as I know, winit's Wayland path dlopens only `libwayland-client.so.0`. Debian ships `libwayland-cursor0` as its own package. Inferred, not traced:
  - **Drop it**, unless the lane shows (`LD_DEBUG=libs` or `strace -e openat` on a Wayland box) that the game opens it.
- **X11 set:** `libX11`, `libX11-xcb`, `libXcursor`, `libXi` and `libxkbcommon-x11` are all opened as required by winit 0.30's X11 connection and xkbcommon-dl. The last one is the reported panic. That list matches (inferred from winit 0.30's XConnection; not re-read).
- **No escape hatch is needed** once the list is exact. A refusal then means winit would have failed too, with a worse message.

## (b) Windows and Mac: unaffected

- `system_libraries` and its call are `#[cfg(target_os = "linux")]`, and `libloading` is a Linux-only dependency. libloading 0.8.9 was already in Cargo.lock through the GPU stack, so no new crate is downloaded. The one lock line is the new edge.
- `backend_order` now intersects with `Instance::enabled_backend_features()`.
  - On Windows the first entry is still DX12 (Metal was never built there).
  - On Mac it is still Metal.
  - The `WGPU_BACKEND` env override is untouched.
- The only Windows-visible change is the error text: `NO_GPU_HELP` is added as context in front of "No usable GPU backend: ...". Nothing matches on that string (searched).

## (c) Dialogs degrade cleanly: yes

- The message always goes to stderr first.
- With no `DISPLAY` or `WAYLAND_DISPLAY`, there's no dialog. With neither tool installed, `Command::new` fails, the code moves to the next tool, and finally returns false.
- zenity uses `--no-markup`. kdialog may render HTML-looking text as rich text, which is harmless.
- `launch.sh`'s `tell` does the same and is silent with a terminal attached.

## (d) libloading: justified, and contained

- A simpler check (looking for files in `/usr/lib*`) would get Flatpak, the Steam runtime, multiarch paths and `LD_LIBRARY_PATH` wrong. dlopen is the only check that answers exactly what winit will see.
- The single `unsafe` is `Library::new`, with a SAFETY note. The handle drops at once, and these are libraries the process loads next anyway. Nothing else is unsafe.

## (e) Exec bit: the release path was already right, and the launcher now covers extractors

- `tools/package_playtest.sh` already writes mode 0755 into the zip for `bri-client`, `bri-import-addon`, `bri-server` and `launch.sh` (~65, ~173). The zip carries the bit, and extractors that ignore Unix modes drop it.
- The launcher now `chmod +x`es the three binaries when needed. It can't fix **itself**: if an extractor dropped `launch.sh`'s bit, a double-click does nothing.
  - **Ask:** one line in README and TESTER-GUIDE: "If launch.sh won't start, run `bash launch.sh` in the folder."
- glibc: `./bri-client --version` runs first, so a loader `GLIBC_x.y not found` becomes a plain message with the needed and present versions. Good.
- The tee pipeline keeps the client's exit code (`PIPESTATUS[0]` through the brace group). Each stream goes to the terminal and its own log.

## Small

- TESTER-GUIDE still says that with no GPU "the game falls back to slow software drawing". On Linux that's true only if a software Vulkan driver (lavapipe, `mesa-vulkan-drivers`) is installed, since there's no GL build. Say so in that sentence.

## Follow-up fa3113f: passes, merge exactly fa3113f

- **The lane's pushback is correct.** I read winit 0.30.13 (crates.io source), `src/platform_impl/linux/mod.rs` ~734-775. With WAYLAND_DISPLAY or WAYLAND_SOCKET set (non-empty), it picks `Backend::Wayland` and returns `new_wayland_any_thread()`'s result with no X11 retry. The project never forces a backend (no `with_x11` or `with_wayland` in crates). So "a Wayland session needs the Wayland libraries" is exactly what will be loaded, and my item 1 is withdrawn.
  - Small difference, harmless: winit treats an empty WAYLAND_DISPLAY plus a set WAYLAND_SOCKET slightly differently through its or_else/filter. `wayland_session` uses "either is set and non-empty", which gives the same answer in every real case.
- `libwayland-cursor` dropped, with the reason in the doc comment. The `bash launch.sh` and lavapipe doc lines are in.
