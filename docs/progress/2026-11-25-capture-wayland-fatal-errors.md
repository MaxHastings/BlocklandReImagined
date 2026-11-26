# 2026-11-25 Capturing the reason for mid-session Wayland connection loss

Phase 2 of the "closes and restarts fullscreen" bug. Phase 1 (this branch,
commit `62415e0`) stopped the silent X11 re-exec: a display loss more than 5 s
into a session now ends in the "could not continue — the connection to the
display server was lost after Ns" dialog instead of a restart. Maxwell saw
that dialog on his test laptop (32 s into a session), so the remaining question
is *why* the Wayland connection dies mid-session — and the answer has never
been captured: every death logged only winit's bare `Exit Failure: 1`.

## Why the reason was invisible

- winit 0.30's Wayland backend sets exit code 1 on a failed
  `connection.flush()` (hardcoded) or on a dispatch error without an OS error
  code, then returns `EventLoopError::ExitFailure(code)` — no message travels
  with it. Code 1 therefore means "flush failed" *or* "non-OS dispatch error";
  only the stack that owns the socket knows which.
- The actual reason is logged by wayland-backend at the moment of failure
  (`store_and_return_error` → `log_error!`), and libwayland's C layer reports
  protocol errors through a trampoline that wayland-backend installs on every
  client connection when its `log` feature is enabled. Without the feature,
  both paths fall back to bare `eprintln!`.
- Our build did not enable that feature: winit enables only `client_system`,
  and wayland-backend has no default features at all (verified with
  `cargo tree -i wayland-backend -e features`: client_system, an empty
  `default` from sctk's plain dependency, dlopen — never `log`). So every
  fatal error went to stderr, which a game started from a file manager never
  shows. Nothing in the game installs a `log` logger either (verified).
- Even the tracing half had nowhere persistent to go on Linux:
  `bri_crash::finish()` is a no-op and the session log file is created
  header-only (`drop(log)`), so `logs/session-*.log` never received console or
  stderr lines.

## The change (branch bionic/display-loss-no-relaunch)

- `crates/client/Cargo.toml`: Linux-only deps — `wayland-backend = { version =
  "0.3", features = ["log"] }` (cargo feature unification turns the feature on
  for winit's and sctk's copies of the same locked crate; no new versions) and
  `log = { version = "0.4", features = ["std"] }`.
- `crates/client/src/winit_log.rs`: alongside the existing tracing forwarder,
  installs a `log` facade logger (`DisplayLog`) that forwards WARN/ERROR records
  from targets starting with wayland/winit/smithay to `bri_console`, and
  appends each line — in stderr's exact "Error: …"/"Warning: …" form — to this
  run's session log file, so it survives both the window closing and a launcher
  that discards stderr. If another crate had already installed a logger we keep
  theirs (none does today).
- `crates/crash/src/lib.rs`: new accessor `session_log_path()` exposing the
  existing per-run session log path (no new state or file kinds).

## Checks

`python3 tools/check.py focused --crates bri-client,bri-crash`: fmt clean,
clippy `-D warnings` clean, unit tests 816 passed; 5 offscreen-render failures
(`wgpu error: Out of Memory`) are the known VRAM-contention flakes — each
passes alone with `--test-threads=1`. New tests:
`a_display_record_is_a_warning_or_error_from_the_display_stack`,
`a_reported_line_is_appended_to_the_session_log_file`.

## Next (needs Maxwell's playtest)

One more failing session on the test laptop — sit in the start-game Add-On
selection until it dies. The newest `target/release/logs/session-*.log` will now
contain a line naming the failure, and that line decides what this is:

- `Error: wayland_backend::…: Connection reset by peer (os error 104)` /
  "Broken pipe" → KWin closed the socket; compositor/driver side. Compare with
  kwin_wayland's log at that second, and try a control launch under X11
  (`env -u WAYLAND_DISPLAY`) or another session to confirm.
- A protocol-error line (e.g. "Client protocol error: … interface.opcode") →
  a bug in our own client code; the message names the interface and opcode to
  fix.

Also wanted from Maxwell: the test laptop's GPU, whether it runs Wayland too,
and how he launches the game — with this build, running from a terminal shows
the captured line live on stderr as well.
