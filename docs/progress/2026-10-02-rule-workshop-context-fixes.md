# 2026-10-02 Rule Workshop context and lifetime follow-up

Continues `rewrite/rule-workshop`; preserves the existing spike. Review before
handoff exposed three concrete creator behaviors that needed correction:

- A timer could increment MiniGame state but its changed input lost MiniGame
  context. Changed inputs now retain captured match/object context, using the
  existing deferred event path. A two-second timer reaction without a player
  now ends the real round in a headless test.
- A named-target brick state write notified the source switch rather than the
  changed brick. Brick-scoped writes now notify the changed target; other scopes
  notify the source. A cross-brick reaction test verifies the state and color
  change through the same scheduler.
- Deleted-brick counters were reclaimed only while regions were active.
  Once-per-second cleanup now runs in pure switch/puzzle worlds too. Missing
  entities cannot supply even default-zero variable values; the real ball reset
  test covers the old object identity after replacement.

No generalized cause bus, state platform or second interpreter was added.
The sixteen spike tests all pass. The full events/UI/sim/world suite was rerun:
860 passed, zero failures, 136 ignored. Clippy with warnings denied passes on
the affected sim/client/net paths. The earlier network sweep also passed:
93 tests across net library, loopback, event storms and state limits (7 ignored).
Native v20 events, editor field flow and actual-pack offscreen rendering passed
as recorded in the preceding entry. No interactive gameplay was automated.

Documented another material design choice: swept region detection currently
treats teleports/portal jumps as straight segments, so intermediate sensors may
fire. This is deliberately exposed for creator judgment.

The branch's Windows launcher now supplies the existing explicit state-directory
argument, using `%LOCALAPPDATA%\BlocklandReImagined-RuleWorkshop`. Settings/saves
from the experiment stay separate from the normal profile. Double-clicking the
executable itself still uses the normal profile; the playtest guide says to use
`Launch.cmd`. Original installations remain unchanged.

Windows packaging run 37048646186 builds the initial spike. Final handoff must
use a subsequent artifact containing these fixes and the isolated launcher.
No public release or main merge is authorized by this work.
