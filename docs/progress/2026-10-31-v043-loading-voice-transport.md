# Accepted loading voice transport

Continuation of the accepted connection identity and explicit optional-origin
increments. Arrival now owns a normal VoiceLink with bounded audio/activity
inboxes, a datagram reader and private reliable route updates. Those readers run
independently of UI polling. No microphone is opened by accepting a connection.

The host voice participant view borrows both gameplay peers and accepted lobbies,
preferring the current gameplay transport during replacement. It allocates no
combined peer map per audio packet. Route generation binds to the same verified
owner, destination/game/team and current transport. Audio/activity events must
match that transport generation. Both connection kinds use one shared voice
admission implementation with the existing separate rate budgets, capture-clock
sequence ordering, age checks and drop-on-full authority queue. The lobby has no
movement datagram or gameplay command path.

Voice mute/unmute now resolves accepted humans as well as embodied peers and
updates every active connection sharing a verified principal. Admission already
restores principal mutes through the existing administration path. Policy and
personal mute ownership remain unchanged. A protocol marker records that the
accepted lifecycle now permits voice routes and datagrams.

Initial type checks found missing parentheses around struct expressions in two
server guards; those were corrected. The fixture initially used a raw owner as
an admin ConnectionId and a nonexistent Cache constructor; it now uses the actual
public APIs. The production all-target net type check passes. The first real QUIC run passed all five loading/chat, five voice-transport and
thirteen policy cases, including new bidirectional gameplay/loading and
loading/loading speech, activity, no-body identity, game isolation, accepted
mute/unmute, fresh grants on gameplay entry and an authenticated reconnect to a
retained game isolated from a separate loading lobby participant.

The fourteenth policy case failed after map change: a loading client retained
its old route when the new map had no voice provider. The host had erased sent
route tables before the refresh could send revocations. Preserve those tables
until refresh, invalidate the prior conversation context instead, and issue
fresh grants/revocations through the existing path. The red deadline failure is
preserved as `loading-voice-map-revocation-red.txt`. The unchanged assertion now passes after that correction. The complete
corrected-source scoped run passes all 26 cases: five loading/chat, five voice,
fourteen voice-policy and two protocol-marker checks. This is a supported
red/green regression, not a flaky retry or weakened assertion. Corrected-source
all-target sim/net/client Clippy passes warning-free as well.

Commands: `cargo check --locked --profile playtest -p bri-net --all-targets`;
`cargo test --locked --profile playtest -p bri-net --test voice_policy --test voice
--test chat_while_joining --test protocol_changes`; and `cargo clippy --locked
--profile playtest -p bri-net -p bri-sim -p bri-client --all-targets -- -D warnings`.
`cargo test --locked --profile playtest -p bri-sim --lib
session::connections::tests` also passes both authorization/reconnect/map-ID cases
on the corrected source (28 relevant passing cases total). These are scoped
checks, not a full workspace/release or platform-CI pass. Logs use `loading-voice-transport-*.txt` under
the established private diagnostics folder.

Still open: frame-independent capture/send/playout pumping, client loading UI
conversation/mute metadata, stable audio-device ownership through accepted-to-body
transport replacement and combined loading-pressure/route lifecycle acceptance.
A transport test is not microphone/device or interactive gameplay acceptance.
Maxwell's waiver of further playtests stands. No release publication is claimed.
