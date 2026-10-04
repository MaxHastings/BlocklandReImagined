# 2026-10-03 Administration refresh ordering

Maxwell reported that opening administration sometimes leaves every action
disabled with “Waiting for authoritative state” until reconnecting or restarting.

The local source has a reproducible ordering race. A correlated command reply
can deliver revision 5 to the UI before the worker's cached view publishes it.
Opening administration then refreshes from cached revision 4. The model correctly
rejects the stale state, but the local success acknowledgement keeps Refresh
pending and all administration buttons busy. There was no server permission
failure in this reproduction.

Refresh now chooses the newer of the authenticated connection view and the
already validated UI snapshot for this same session. Applying that state clears
Refresh before its acknowledgement. Disconnect/new connection reset the UI's
administration model, and session tokens still reject answers from older joins.
Actual commands retain server permission checks and normal request deadlines.
This does not grant authority locally or accept a stale demotion rollback.

Evidence:

- Intended baseline failure: `/tmp/bri-v023-admin-refresh-before.log`.
- Full client administration adapter suite: 13 passed,
  `/tmp/bri-v023-admin-refresh-final-3.log`. Covers cached older/newer/equal,
  missing UI state, live host capabilities, and a newer demotion.
- UI administration screen suite: 15 passed, 2 content-dependent ignored,
  `/tmp/bri-v023-admin-screens.log`.
- Both existing acknowledgement/list deadline regressions passed,
  `/tmp/bri-v023-admin-request-timeouts.log`.
- Independent stability-agent audit found the snapshot/session boundaries and
  acknowledgement ordering consistent.

The first added positive capability control lacked Maps support in its fixture;
the following correction used the wrong capability variant. Both failed receipts
are preserved (`admin-refresh-final.log` and `admin-refresh-final-2.log`); the
final fixture explicitly advertises ChangeMap. No production permission checks
were weakened to satisfy it.

Commands used `CARGO_BUILD_JOBS=2 cargo test --locked`: client `--lib
admin_ui::tests`, UI `--test admin_screens`, and UI `--lib an_admin_`, each
with `-- --test-threads=1` under the single compute lease.

This fixes the demonstrated refresh race locally. Maxwell's exact session has
not been reproduced interactively, and an indefinite network/server stall is
not established by this test. Publication and human playtest acceptance remain
pending with the rest of v0.2.3.
