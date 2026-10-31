# Accepted loading conversation identity

The v0.4.3 loading voice/chat increment now derives an authorized destination
from accepted human identity before a body exists. The existing authoritative
membership journal retains verified reconnect grants; explicit membership removal
revokes them and map adoption leaves the journal behind. Pending destination is
retained game, host automatic server/default game, then lobby. Existing default
entry is an explicit host grant, including an invite-only default; arbitrary
private games confer no access. Entry uses the same retained destination.

The live read-only package `connection(owner)` query exposes accepted human
identity, loading state and destination separately from body-only `player()`.
Loading team, life state and coordinates are absent, with no invented origin,
readiness, game membership, vitals or team slot. Stock voice policy selects
nonpositional loading routes within the authorized scope, denies unknown teams
and dead-channel access, and preserves all-member Global behavior. The sample
radio explicitly denies private transmit to an unknown loading channel so a
public provider cannot become a fallback.

Scoped game notices and kill feeds use the same audience; existing accepted
QUIC notice delivery now forwards kills as well as chat/sounds. The client already
handles loading Kill notices. No new wire/save representation is introduced.
The package voice seam and modding guide document the new view and its limits.

Verification so far: `cargo check --locked --profile playtest -p bri-sim --lib
--tests` and `cargo test --locked --profile playtest -p bri-sim --lib
session::connections::tests` pass (two cases). They exercise authorization before
body creation, chat history exclusion, private-game isolation, live destination
switch/revocation, dead-versus-Global policy, exact entry, authenticated versus
same-name reconnect, explicit kick and reused game IDs on map replacement.
Initial test compilation found an unsupported Diagnostic-to-anyhow conversion
and a private snapshot helper call. The fixture now formats diagnostics and
uses the direct no-peer/vitals/minigame-player assertions; production query
visibility was not widened for tests.

`cargo check --locked --profile playtest -p bri-net --test chat_while_joining`
passes. `cargo test --locked --profile playtest -p bri-net --test
chat_while_joining --test voice_policy` passes all five chat/arrival and twelve
voice-policy cases. The new actual QUIC case creates a normal player-owned game,
sets the default through a declared package minigame operation, holds an accepted
arrival without a body and receives another member's game-scoped join notice.
The sample-radio loading denial also runs without script diagnostics. Existing
transport replacement, impersonation prevention, typing/roster cleanup and
conversation-generation cases remain green. `cargo clippy --locked --profile playtest -p bri-package-runtime -p bri-sim
-p bri-net -p bri-client --all-targets -- -D warnings` passes. These are scoped
checks, not a full workspace/release gate.
Private command logs live under the established release-evidence diagnostics
folder as `loading-conversation-*.txt`.

Still open: actual arrival voice transport, optional spatial origin end-to-end,
background transport/audio pumping during frame stalls, loading conversation UI
metadata and continuous audio across accepted-to-body connection replacement.
This is an authorization/query foundation, not completed loading VoIP or final
release acceptance. No interactive playtest was performed or requested.
