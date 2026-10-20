> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-mac-windows-launch.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: Mac and Windows launch fixes (fa3113f..ee04184, claude/fix-linux-boot-c5rao2)

Verdict: **passes. Merge exactly ee04184 once the Windows CI build on this branch is green and the import check prints "no Visual C++ Redistributable DLLs" for all three exes.** That run is the first link of crt-static. Until then the Windows half is unproven.

## (a) crt-static: safe with what we link (inferred; the CI link proves it)

- **Rust std:** `+crt-static` is a supported, common setting on `x86_64-pc-windows-msvc`.
- **C code built through `cc`:** `cc` reads `CARGO_CFG_TARGET_FEATURE` and compiles with `/MT` when crt-static is on, so its objects match. The lane says `cc` is the only C build dependency.
- **The real risk** would be a `-sys` crate linking a **prebuilt** `/MD` `.lib`. That fails to link or warns (LNK4098) instead of breaking at run time, so the CI build will catch it.
- **DLL boundaries:**
  - `dxcompiler.dll` and `dxil.dll` are reached through COM-style interfaces whose blobs are freed by their own `Release`, never by our CRT. Mixed CRTs are fine there.
  - Add-On WASM runs inside our process through the wasm runtime. Nothing is a separate native DLL with its own CRT.
  - Proc-macros (built for the host with the same rustflags when not cross-compiling) talk to rustc through proc_macro's bridge, which owns its buffers. That is the standard crt-static case.
  - No mixed-CRT problem that I can see.
- `ucrtbase` and `api-ms-win-crt-*` are part of Windows 10/11 anyway, and with crt-static even those are static.

## (b) The root .cargo/config.toml

- **Linux and Mac:** unaffected. It's a `[target.x86_64-pc-windows-msvc]` table only.
- **The PC gate and Max's dev builds** get it too, which is right (they test what ships). The flag is part of every Windows compile key, so **the first gate and the first dev build after merging rebuild everything once**, with sccache misses for all crates. Expect one slow gate. sccache itself is unaffected (rustflags are hashed in like any flag), and `rustc-wrapper` in Max's user config merges with this file.
- **Two traps worth one line in the config comment or docs:**
  1. A `RUSTFLAGS` env var **replaces** this setting silently. The comment says so. The import check in CI and release catches a lost flag, but a local PC build wouldn't. Fine.
  2. Cargo uses only one rustflags source. If Max's `~/.cargo/config.toml` ever adds `build.rustflags` (for example `target-cpu=native`), the repo's `target.*.rustflags` wins and those flags are dropped, or the other way round depending on the key. Today his config has only `rustc-wrapper` (AGENTS.md), so nothing changes now.

## (c) osascript: injection-safe

- The script is three fixed `-e` lines, `on run argv` / `display alert (item 1 of argv) message (item 2 of argv) ...` / `end run`. The title and crash text are separate argv items after the script, so they are never parsed as AppleScript.
- The only interpolation is the `OPEN_LOGS` constant into the buttons list.
- The test feeds a `"` plus `end run` payload.
- Reading `button returned:Open Logs` from stdout is fine.
- `open` replaces `xdg-open` on macOS, which fixes Open Add-Ons, saves and the update page, which all silently did nothing on Mac before. That's a real player-facing fix.

## (d) Import checker: sound

- It's a denylist of the redistributable's DLL prefixes (`vcruntime`, `msvcp`, `concrt`, `vccorlib`). It can only fail on those, never on a system DLL.
- PE32+ parsing:
  - section mapping by virtual address and size;
  - descriptors read up to the all-zero terminator (it checks the name RVA, which is zero in the terminator);
  - a clear exit on a non-PE or 32-bit file.
- **Small, optional:** it reads only the normal import table, not the delay-load table. The VC runtime is never delay-loaded by Rust or `cc` builds, so this is fine. Adding `vcomp` (OpenMP) and `msvcr` (old CRTs) to the prefixes costs nothing.
- It runs after Build in ci.yml (debug exes) and after the release build in release.yml. Both are good places.

## (e) The friend's Mac report ("did not work even after allowing it through security")

**What this addresses:** a startup failure is no longer silent. The alert says why and opens the logs. The docs now say Intel Macs can't run it.

**Checked by the lane, so not the cause:** the ad-hoc signature's seal covers all 9,415 files; minimum macOS 11.0 matches the plist; content lives inside the bundle (`Contents/Resources/content`, main.rs ~235-267), so App Translocation's read-only random path doesn't hide it; and the first-run copy goes to Application Support.

**Still unaddressed or unknown, in likelihood order (inferred):**
1. **An Intel Mac.** The build is arm64 only. macOS says "not supported on this Mac", which a player could describe as "didn't work after allowing it". It's now documented, but a universal build is a v0.2.7 or Max decision.
2. **macOS older than 11.**
3. **A real startup error** (GPU, the first-run 600 MB content copy, permissions). Unknown until the friend runs this build and sends the alert text or log.
4. **Notarization:** none. On macOS 15, "Open Anyway" still works for ad-hoc apps, so it isn't a blocker, but every first launch needs that dance. Notarizing needs Max's paid Apple account.

**Ask:** the friend's Mac model (Apple menu → About This Mac: chip and macOS version), plus the alert text from this build. That settles it.
