# 2026-10-31 Persistent minigame ownership

Current dependency/acceptance handoff for creator departure. The initial audit
found that Leave/disconnect ended the creator's encounter, bot/vehicle affiliation
followed their participation, and the UI refused creator Leave. The ownership,
resource and UI corrections below are now implemented with scoped evidence;
remaining combined persistence/mode/display acceptance is named at the end.
Changing only Leave would not have preserved a running battle.

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


## Resource affiliation correction — scoped checks complete

Stock creator-owned brick groups now resolve to the creator's retained game
before the creator's current participation. This applies to new placements too:
all their bricks/spawners belong to that encounter until explicit End. A body
joining another game changes combat permissions, not ownership of the original
encounter. Existing explicit per-object membership remains available; this is
not a new general per-brick identity format. Offline groups belonging to a
retained encounter cannot be adopted into another encounter through Full trust.

Creator Leave/rejoin/disconnect no longer cancels their event schedules or
replaces their running vehicles. Resource cleanup carries an AccountId, not a
PlayerId requiring a connected body. Initial creation initializes resources once;
Reset/End include absent creators. Reset excludes resources belonging to another
member's independently owned encounter. Bot membership, vehicle return/respawn
policy, damage/use resolution, script brick views, native region observation,
package game broadcasts and player-triggered MiniGame event targets consume the
resource context. Actual player combat and event instigator permissions continue
to use actual membership.

Evidence obtained so far for this increment:

- Domain full suite: 40 pass (28 rules, 12 teams), including reset/end cleanup for
  an offline creator and rejoining without resource reset. The resource-affiliation
  regression failed before the correction.
- The headless Session battle regression failed with bot game None before the
  correction, then passes: the same bot life remains in its original game and
  ordinary AI controls/melee damage the remaining player after the creator leaves
  and joins another game. Creator disconnect retains its affiliation. This is one
  synthetic encounter, not a networked crowd or restarted-world claim.
- The native reset-brick regression fails after creator Leave before correcting
  the event target lookup, then passes both connected-outside and disconnected
  creator states. All 11 ordinary v20 event cases pass; the two pre-existing
  generated-content cases remain separately available, not counted as passes.
  The new fixture first reused a command sequence; correcting its sequence gave
  the expected behavioral failure. No assertion was relaxed.
- All-target sim/net/client type checking passed before adding the final queued
  event and autonomous-message cases; those cases and broader checks are running.

Private diagnostics: `minigame-resource-{domain,types}.txt`,
`minigame-battle-{red,green}.txt`, `minigame-events-resource-{red,green}.txt`
and `minigame-autonomous-resource-red.txt` under the planning diagnostics folder.
Resource/UI/network combined acceptance remains open, including autonomous events
without a connected creator, item deadlines, new placement/save/restarted world,
server/default/Slayer behavior and final multi-client continuing damage. Keep the
feature unchecked until that chain and the usable departure/rejoin UI are complete.


The final autonomous round-message regression exposes another connected-body
assumption: an authored MiniGame output with no instigator is rejected after its
creator disconnects. Add source-brick-validated `EventAuthority::AutonomousBrick`
carrying only the authoritative builder account. It is limited to that account's
owned game; it neither grants general System authority nor invents a player for
name/score substitutions. Missing source bricks are rejected rather than treated
as world owner 0. A domain regression rejects a participant's and the server's
accounts for somebody else's retained game. The real native rule-message case
must pass before this increment is committed. No wire/save representation changes
are introduced by this internal command/effect correction.


Next UI chain, once resource checks pass: derive the owned game from the existing
stable local owner selector in `MiniGameUiState` rather than add a second registry.
`models/minigames.rs` currently suppresses creator Leave; the list's Leave handler
instead offers End, and the Escape menu skips the list for an active creator.
These paths need one consistent overview with distinct Leave, My Minigame/Manage,
Rejoin and explicit End. Default settings/draft targets and client action adaptation
must resolve the owned game independently of active membership; preserve explicitly
selected administrator-editable targets. Invite-only filtering must allow a creator
to rejoin their own game. Explain that creator-owned brick/spawner groups keep
running until End, including placements made while outside. Verify actual UI actions,
host denial and network membership/resource observations; no interactive playtest.


The autonomous message now passes in all 45 Session rule cases, together with
queued score/reset events and a retained vehicle through departure/disconnect.
The candidate's initial generic owner lookup also incorrectly selected the first
shared SERVER-owned game for world owner 0. A new domain case fails that behavior.
Creator priority now applies only to non-server games; server resources retain
exclusive-game and authored claiming rules, independent of default participation.
All 42 domain cases pass (30 rules, 12 teams). Final all-target warning-free Clippy
passes for minigames/sim/net/client. Changed-area native generated-content, saved
build and network consumers are being run against this final resource correction;
retain the final receipts below before considering this increment verified.


Final resource-increment receipt: **113 distinct scoped cases pass**, no retries:
42 minigame domain cases; 45 Session rules; two bot spawn/lifecycle cases; all 13
v20 event cases including both generated-content cases; both synthetic and native
converted-content bot-spawner combat cases; two actual QUIC minigame cases; five
native saved-build/direct duplicator consumers; and both protocol history cases.
Overlap with the earlier foundation receipt is not an additional 113 features.
Commands use `cargo test --locked --profile playtest`: domain selects
`-p bri-minigames`; all remaining commands select `-p bri-sim -p bri-net
-p bri-client` and respectively `--lib session::rules::tests`, `--lib spawn_tests`,
`--test v20_events -- --include-ignored`, `--test vehicles
bot_brick_spawns_a_bot_that_fights_inside_its_owners_minigame -- --include-ignored`,
`--test loopback minigame`, `--test script_api --test advanced_duplicator save`
and `--test protocol_changes`. Final logs are `minigame-resource-*-final.txt`.
Final all-target warning-free Clippy and diff whitespace checks pass. These are
changed-area checks, not final CI/platform/full-release acceptance. No human
playtest is claimed. Next: implement the UI chain above, then complete actual
network continuing-resource and native persistence/default/Slayer acceptance.


## Creator controls and combined network battle — scoped checks complete

The UI derives its owned-game selector from existing authenticated summaries,
independently of active membership. Escape opens the overview. Creator Leave sends
Leave, My Minigame opens owned settings, and Rejoin admits the creator to their
invite-only game using normal spawning. End remains a separate settings action.
The overview explains that Leave keeps the game running; Basics explains that the
creator's bricks/spawners stay with it until End. Successful list refresh no longer
replaces that explanation with a generic completed message, caught by actual
source-art inspection and a failing regression before the correction.

Owned defaults apply to settings, pending post-create Add-On drafts and client
management adaptation. F2 Invite/Remove use owned encounter membership/identity;
list highlighting/scores still describe actual participation. Explicitly selected
editor targets stay explicit, including a new optional game selector on the
client-only InviteMiniGame UiAction, consumed through the existing host-authorized
Manage request. Shared SERVER-owned games allow ordinary join/leave/create;
exclusive game-mode restrictions remain. No identity registry, new wire message,
wire/save representation or migration is introduced in this UI increment.

Final evidence: **65 distinct scoped cases pass**, plus **nine offscreen frames**:

- UI model: three cases (`cargo test --locked --profile playtest -p bri-ui --lib
  models::minigames`).
- MiniGame screen integration: 44 ordinary cases (`-p bri-ui --test
  minigame_screens`); the pre-existing broad GPU matrix is separate and not
  counted. Includes creator departure/private rejoin, outside/other-game settings,
  host Leave revocation, F2 owned membership, explicit editor invitation and the
  listing explanation.
- Field-flow consumers: all eight synthetic/converted-content cases (`-p bri-ui
  --test field_flow -- --include-ignored`).
- Client bridge: six, actual QUIC: two, Session saved-minigame/rule consumers: two
  (`cargo test --locked --profile playtest -p bri-sim -p bri-net -p bri-client
  --lib --test loopback minigame`). The final expanded QUIC case also passes in
  `--test loopback minigame` after adding normal rejoin/zero-human checks.
- Warning-free all-target Clippy for ui/client/net and diff whitespace pass.
- `cargo run --locked --profile playtest -p bri-ui --bin ui_runtime_probe --
  content/ui-pack-005 <private-capture-folder> --minigame-review` renders three
  screens at three sizes/scales. Real source-pack hit testing reaches the overview
  from Escape, sends Leave, and opens retained settings through My Minigame. Final
  1024x768 overview and 1920x1080 at 2x settings are visually inspected: new labels
  fit and the ownership explanation remains visible after list completion. The
  fixture has coherent invite-only/catalog/member metadata. No window or OS input.

The QUIC case uses a normally planted/wrenched melee bot, observes new damage at
least a full simulation second after both departure and disconnect transitions,
and retains the same bot spawn identity until explicit Reset. The outside creator
remains alive/unhurt. A member cannot reset it; the resumed creator reclaims the
same account/game, manages it without joining, then rejoins through a fresh normal
spawn. Both humans can Leave while the bot keeps its affiliation; End removes the
game and bot affiliation. The stopped server reports zero simulation-step errors
and no failure. This is actual hosted multi-client combat, not a policy-only test.

Retained failures: the three initial UI behaviors and the F2/list-explanation cases
fail before correction. The F2 fixture initially omitted required Players-update
metadata. The initial UI edit retained an invitation Callback use after removing
its import and hit a collapsible-if lint; both are corrected. An early combined
Cargo filter selected zero integration cases; those are not counted and the actual
integration targets were run. The expanded network fixture initially requested
items absent from its tool-only pack; its starting/configured equipment now uses
that pack. Its default-spawn actors could overlap and occlude combat; the member
now walks clear using ordinary input paced six moves per 50 ms (120 Hz), rather
than instant repeated batches that exceed normal admission. No damage assertion,
permission, input limit or timeout is relaxed. Initial 0.5-second movement was not
assumed to cover a fixed distance: the bounded control follows observed position.

Private evidence is `minigame-departure-*-final.txt`,
`minigame-departure-combat-{diagnostic,green,complete}.txt`,
`minigame-listing-explanation-red.txt`, `minigame-f2-management-red.txt` and
`minigame-departure-ui-captures/` under planning diagnostics. Earlier red logs and
pre-final green receipts remain separately identifiable. These checks overlap
prior foundation/resource coverage; counts must not be summed as unique features.

Next supported correction: complete the explicit native restart/save-load resource
case and the server/default/Slayer lifecycle combinations, and ensure offline/late
join creator display uses existing verified owner-name metadata. Current public
world replication omits OwnerRecords; do not invent an identity/name registry.
This does not complete step 4's remaining loading PCM continuity or final release
CI/history/protocol/platform/package gates. Maxwell waived further playtests; no
new human acceptance is claimed.
