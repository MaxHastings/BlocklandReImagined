# Voice control independent of world backpressure

Critical private voice control now uses one server-initiated reliable QUIC
unidirectional stream instead of world Message::VoiceRoutes. A watch channel
coalesces pending snapshots while one bounded frame is in flight. The stream has
higher send priority and a 64 KiB uncompressed MessagePack limit. It never uses
the world codec's larger decompression budget. The client permits exactly one
server uni stream; the host continues permitting no client uni streams. Client
stream receive windows are 2 MiB each with 8 MiB connection credit, so one stopped
world stream cannot exhaust credit for the separate voice stream. This is not a
claim of unlimited bandwidth or no physical network latency.

Control includes a monotonic snapshot revision, generation-stamped playback
routes and explicit enabled/moderation/authorized conversation metadata. The
conversation uses the same accepted-connection destination as scoped loading
chat; it adds no membership, pose or team rights. No-voice hosts retain route
revision zero while publishing disabled state and the native conversation name.
Metadata-only changes do not restart unchanged route decoders.

VoiceLink starts its independent control reader before initial world chunk
assembly. A shared reader lease aborts it when the last link is dropped, including
failed joins, without a reference cycle. Frames validate bounds, generations,
finite playback and conversation text before becoming one coherent snapshot.
Stopping/resetting the control stream retires voice grants immediately. Ordinary
teardown leaves the world's connection reader to preserve the actual kick/drop
reason; malformed control remains a protocol violation. Peer removal aborts its
publisher. Map replacement explicitly revokes or rekeys through the retained
sent-state path.

The native voice worker consumes enabled state, moderation and routes from one
snapshot. It no longer takes delayed moderation writes from the frame's world
replica. Capture cannot start before an enabled snapshot. Existing device/opener
ownership can stay warm during a same-session pending/replacement voice link,
with transmission gated off and old grants purged; that state cannot start a new
device or carry authorization to a different session. Explicit disabled control
closes voice capture as before.

Evidence so far: type checking and the initial changed-area Clippy pass. All 26
existing loading/chat/voice/policy/protocol integration cases pass on the new
stream. The new actual QUIC pressure case observes a completely full bounded
world receive queue, makes no paused client's receive/UI calls, then verifies
route revocation and metadata-only mute/unmute delivery while the world queue
stays full. It passes in 1.54 seconds. The initial fixture had one spawn point for
two players and correctly failed native obstruction validation; separated valid
spawn points fix the fixture without changing the pressure assertions.

Final-source checks pass: 12 client cases selected by `--lib voice`, eight net
unit cases (including actual pressure and strict control-codec bounds), and 27
loading/chat/voice/policy/protocol integration cases, 47 total. The added
kick-reason case confirms normal teardown keeps the actual admin reason and
retires the ended link's voice grants. The retained-game case additionally
asserts the published conversation ID matches its authorized destination; that
case passes after adding the metadata assertion. Control-codec cases reject
trailing bytes, oversized input and compressed world frames. No retry-only
passes or weakened assertions occurred.

Commands: `cargo check --locked --profile playtest -p bri-net -p bri-client
--all-targets`; `cargo test --locked --profile playtest -p bri-net -p bri-client
--lib voice`; `cargo test --locked --profile playtest -p bri-net --test voice
--test voice_policy --test chat_while_joining --test protocol_changes`; and
`cargo clippy --locked --profile playtest -p bri-net -p bri-client -p bri-sim
--all-targets -- -D warnings`. Warning-free final Clippy passes. The final explicit
retained-destination assertion was checked with its one targeted voice_policy
case rather than repeating unrelated suites. Logs use `voice-control-*.txt` in the established
private diagnostics folder. These are scoped transport/control checks, not a
full release, real-device PCM continuity, or interactive acceptance.

Remaining loading handoff: install the accepted link before expensive resource
preparation, publish the gameplay link early enough to keep the same worker
through world assembly, wire loading conversation/roster/moderation UI, preserve
ownership across Add-On reload, and verify device-free PCM under stalls and
conversation changes. Upstream audio currently carries sequence/payload without
a source conversation epoch; explicitly challenge captured old-conversation
frames across a channel/game switch before claiming complete privacy/continuity.
Maxwell's no-further-playtests instruction remains in force. No publication or
broader v0.4.3 completion is claimed.
