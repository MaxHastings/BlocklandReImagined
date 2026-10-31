# 2026-10-31 Persistent minigame ownership

Current dependency/acceptance handoff for creator departure. Leaving currently
ends an owned encounter in `MinigamesWorld::remove_member`; disconnect uses the
same path. Bot and vehicle membership separately follows the builder's current
membership through `Session::spawn_brick_game`. The UI additionally refuses Leave
for owners. Changing only the UI would therefore end the battle or move its bots
out of combat. All three layers must be corrected before claiming the feature.

The domain now separates ownership from participation: `MiniGame::owner` is the
stable host-authenticated AccountId rather than a transient PlayerId. Management
finds an owned game independently of membership, still validates the live session,
and denies stale/unrelated callers. Departure/disconnect retains the game, color,
settings, membership of remaining players and invitations. Reconnection to the same
host-authenticated account regains management without automatically joining combat.
One owned game per account and existing game/color caps remain; zero is reserved
for server ownership. Explicit End retains its cleanup semantics. Owners can rejoin
their invite-only game through ordinary spawning/admission. No dummy actor or
implicit ownership transfer is used.

Trusted live snapshots change to v2, permitting absent/nonmember creators and empty
encounters, rejecting duplicate creators and stale v1 snapshots. Catalog/preset
schemas stay v1: they did not change. Session policy, management, scripts, trust,
objective indexing and native views use stable owner accounts. A protocol marker
records the semantic change. Current typed consumers compile across sim/net/client.

The two creator regressions first fail against the original code at actual Leave
and Disconnect ending the encounter. They pass after the ownership change. The
full minigames run initially flags two v20 assertions that intentionally treated
Leave/Disconnect as End. The old explicit-End lifecycle/admin-denial assertions are
preserved by invoking End; the color/invitation case now additionally requires
retention until an authenticated returning owner explicitly ends it. All 38 domain
cases then pass (26 rules, 12 teams), including the later malformed-snapshot
version/duplicate-owner assertions. During consumer conversion a mechanical
script matched a nested combat.rs by basename and stopped at an assertion after
partial writes; the remaining named consumers were corrected, with no ignored
compiler errors or relaxed assertions.

Wider Session rule checks initially pass 40 and fail two free-build fixtures at
color reuse: their creator Leave formerly meant End. The fixtures now explicitly
End the encounter to establish free-build context; their observation assertions
and the other creator's ordinary Leave remain unchanged. The full rule module
then passes. A new native-save case first fails because `saved_minigame` still
requires participation. Saving/restoring now resolves an owned game before an
editable active game, without joining it or allocating another. Offline recovery
retains the verified principal from the existing world OwnerRecord, and a fresh
verified join reuses the existing `admit` principal-to-world-owner mapping. No new
identity registry is introduced. The new case then passes inside all 43 rules.
This proves retention/reclaim in the running world and offline recovery metadata;
it is not a complete restarted-server encounter/resource acceptance claim.

Final changed-area evidence (94 distinct cases; overlapping reruns counted once):

- `cargo test --locked --profile playtest -p bri-minigames`: 38 pass.
- `cargo test --locked --profile playtest -p bri-sim -p bri-net -p bri-client
  --lib --test loopback minigame`: four client UI bridges, two real QUIC cases
  and one Session rule pass. The new QUIC case proves owner Leave, member denial,
  outside configuration, host-verified resume, reset and explicit End with the
  same game ID and another client observing membership/settings.
- `cargo test --locked --profile playtest -p bri-sim -p bri-net -p bri-client
  --lib session::rules::tests`: 43 pass, including the native-save/reclaim case.
- The same package selection with `--test script_api --test advanced_duplicator
  save`: five direct saved-build consumers pass, including missing/empty/malformed
  per-game state, bounded restoration and actual planted-copy save/load.
- The same package selection with `--test loopback minigame`: both network cases
  pass again after the native-save correction.
- The same package selection with `--test protocol_changes`: two pass.
- `cargo clippy --locked --profile playtest -p bri-minigames -p bri-sim -p bri-net
  -p bri-client --all-targets -- -D warnings`: pass after the final correction.
  All-target consumer type checks and `git diff --check` also pass.

Private diagnostics are `minigame-{owner-*,domain-*}.txt` in the planning evidence
diagnostics directory. These are scoped technical checks, not a full workspace,
GPU, original-reference comparison or final release gate.

NEXT supported correction: bind existing and newly placed encounter resources to
the game independently of their builder's membership, including bot/vehicle/item
sources, event/reset/damage contexts and native save/load. Then enable Leave/My
minigame/Rejoin/Manage UI with clear consequences. Offline owner display metadata,
restarted-server resource reclaim, server/default/Slayer mode behavior and actual
multi-client continuing bot damage are still open. This domain increment alone
DOES NOT prove that a real bot battle continues. Keep the full feature unchecked.
Use only headless/network/offscreen verification: Maxwell waived further playtests.
