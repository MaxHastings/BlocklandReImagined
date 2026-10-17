# Loading chat typing through authority

Root branch `codex/v032-loading-typing`, runtime base published5956. Source is
frozen for final targeted/quick/focused checks; no immutable fix yet.

Reproduced the HUD omission: accepted LoadingChat with an authoritative Talking
name draws exactly the same list as no name (new UI target failed before fix).
Share the existing play typing-name drawing with accepted loading. Target now
passes. Recovered v20 WhoTalkSO and the existing play HUD establish the named
indicator; do not fabricate a local acknowledgement from a typed character.

The App previously ignored StartTyping before network_view existed; Arrival's
wire sent only Chat. Route loading Start/Stop over its authenticated connection,
record both as Session arrival controls, and send authoritative current typing
names to loading connections. Arrival stores ephemeral typing state alongside
its canonical name; enter_arrival transfers that state into the existing peer.
Only changed snapshots are sent to each loading connection, including its initial
empty/nonempty state. Existing queue/request limits and disconnect identity
checks remain. A protocol change marker records the new request/message variants.
Normal in-game vitals behavior is preserved; this is the loading display route,
not new remote gameplay replication.

Real net target first passed 4/0: own and other loading typing, spawned-player
typing, stop/disconnect cleanup, no premature body, existing chat/download/join
controls. A stronger typing-through-spawn test exposed a wrong initial approach:
issuing a new sequence0 command after spawn was refused as stale/replayed. That
attempt was removed. Carry accepted arrival state through Session lifecycle
instead of inventing or consuming a gameplay command number. Enhanced final
target now passes 4/0, including the joined snapshot's typing state.

New held-loading App test uses a real accepted Arrival, ordinary UI chat keys,
App dispatch, actual server response and UI state/draw, then Escape cancellation.
The driver establishes a UI session token and holds loading explicitly; it does
not claim the entire App join pipeline. First actual run passed 1/0 in .57s;
final-source rerun is session79914 alongside final net target. A bare short-name
--exact command was not counted; final commands name the full app::tests path.
An initial ServerOptions Default fixture compile error was corrected to its
explicit fields before the executed test.

Final quick/focused chain is session41480, Cargo2jobs/testthreads8/targetunset,
for bri-net,bri-ui,bri-client,bri-sim and standard dependents. Logs are untracked
target-loading-typing-{ui-red,ui-green,net-final,app-final,quick,focused}.log.
Await actual exits and review final combined diff before committing. No full
integration/main push/publication, physical reporter playtest or source assets.

## Final scoped validation

Session41480 completed EXIT0. Quick: eight steps, 329s, 1295 passed / zero
failed / 94 existing ignored. Full normal focused closure: sixteen steps, 2187s,
225 summaries totaling 2759 passed / zero failed / 288 existing ignored. All
changed-crate formatting and all-target Clippy passed, including normal dependent
integration suites. Final net target4/0 and held-loading App target1/0 passed in
session79914; new UI draw integration also ran within focused. Root reviewed the
source/test/protocol diff and git diff --check. No new ignores or skipped tiers.
These are scoped results; combined candidate, required Windows CI and packaged
release validation remain pending. No private/source content enters the commit.
