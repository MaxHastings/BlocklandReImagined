# 2026-10-15 v0.3.1 WARP readback budget

## Decision

Keep the avatar preview pixel regression and give its readback the full five
minutes already allowed to a client test binary. Other offscreen UI callers
retain the existing 60-second default.

## Evidence

- PR #169 run `38705345352`: Windows build and Clippy passed; shards 1–3
  passed. Shard 0 failed only
  `bri-client/avatar_editor_preview::the_editor_portrait_plays_and_loops_the_authored_run::synthetic`.
- Its isolated retry failed after 64.89 seconds with WGPU's requested wait
  timing out. This is the same software-renderer readback limit seen in the
  prior run, not a failed image assertion.
- `python tools/check.py quick --crates bri-ui,bri-client` passed.
- `cargo test -p bri-client --test avatar_editor_preview -- --nocapture`
  passed locally (1 passed, content-backed case ignored without generated
  v20 content).
- `python tools/check.py focused --crates bri-ui,bri-client` passed all seven
  steps, including Clippy and integration tests.

## Next

Wait for Windows CI on commit `cde7e121`. Merge PR #169 and run the full
release pipeline only after the Windows check is green.
