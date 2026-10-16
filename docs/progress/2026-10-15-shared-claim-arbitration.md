# 2026-10-15 Shared camera and input claim arbitration

The coordinator assigned this slice to the shared engine seam chat on
`codex/shared-claim-arbitration`, based on committed Halo engine `2071dd4f`.
Halo subsequently committed independent crouch work as
`d7bc2d7fb4af48d74a654f10a3c3f2398b91429d`; root will combine it with this
slice, preserving both registry changes. This branch deliberately retains the
assigned base through verification. No other chat's checkout was changed.

## Decision and behavior

The read-only cross-game review confirmed two load-order choices in
`ClientCode`: camera callbacks stopped at the first placement, and each input
bit went to its first claimant. The latter also rebuilt event capture by union
after callbacks. The coordinator approved rejecting contested claims instead of
introducing package priorities.

One resolver now grants only input bits requested by exactly one running Add-On,
at both frame delivery and event capture. Uncontested bits remain independent.
Ownership changes clear captured held input and pending look; no synthetic stock
press is generated when capture ends. An unchanged sole owner retains input.
All eligible camera callbacks run with the same stock camera. Exactly one valid
placement is used; multiple placements retain the stock camera. Inactive and
trapped callbacks supply no placement. Callbacks retain their sandbox budgets
and effects; rejected camera output does not roll back Wasm memory or outputs.
Camera traps and device loss release input capture immediately.

Console conflicts name sorted, bounded claimant IDs, suppress unchanged states
and stop after 32 notices across both channels per game session. Session exit
resets that budget. No manifest, wire format, trust capability or gameplay
authority changed. Root authored the accompanying existing seam-registry row
update and requested its inclusion in this commit.

## Verification

Commands used `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_INCREMENTAL=0` and left
`CARGO_TARGET_DIR` unset. All checks passed; no compile or test failures occurred.

- `cargo test -p bri-client --lib client_code::claims --locked`: nine new
  regressions pass through small synthetic Wasm modules and the real sandbox/
  client boundary. Cases cover reverse load order, disjoint/overlapping bits at
  delivery and capture, held/look transfer, release, frame/camera traps, inactive
  and competing cameras, callback side effects, device loss, session reset and
  bounded diagnostics. The first cold compile took 6m 41s.
- `python tools/check.py quick --crates bri-client,bri-client-sandbox`: four
  steps passed in 146s. Client unit tests: 576 passed, 76 ignored. Sandbox unit
  tests: 14 passed.
- `python tools/check.py focused --crates bri-client,bri-client-sandbox`: six
  steps passed in 263s, including both crates' formatting and all-target Clippy
  with warnings denied, client unit tests and sandbox unit/integration/doc tests.
  Existing ignored cases retain their gate disposition; none were newly ignored.
- `git diff --check`: passed.

After committing, attempted disk-policy cleanup of this worktree's `target/`
using native PowerShell `Remove-Item -LiteralPath ... -Recurse -Force`, preceded
by an absolute-path equality and parent check. Automatic approval review rejected
the command as "blocked by policy" without a more specific reason. No deletion
ran; build output remains. No retry or alternate deletion path was attempted.

No original game content or interactive session was used. Existing bounded
offscreen tests ran normally. The full gate was deliberately left to root's
combined candidate, rather than rerun for this isolated slice.

## Shared backlog and limits

Body-owned crouch remains Halo's atomic slice with AA-private acceptance. AA's
source-frame preservation does not yet establish gameplay attachment or view
semantics. Source's private evidence on `0fea0c0c` demonstrates 32 ms controller
cadence and shader-interface limits; movement/contact ordering, a complete loose
prop lifecycle, guidance and exact material operations are separate proposed
designs, not implementation authorized by this slice. Particle lighting at
`2071dd4f` does not supply those missing material equations.

Root owns registry/integration and the integrated candidate gate. Full private
Halo/AA/Source composition, later consumer pins, Windows CI and Maxwell's
interactive acceptance remain open. No release or alpha acceptance item is
closed by these synthetic checks.
