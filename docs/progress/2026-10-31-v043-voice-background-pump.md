# Voice pump independent of game frames

Move the existing VoiceState packet/device state machine onto one named bounded
worker (`bri-voice-pump`). UI methods supply the latest transport/listener and
synchronous controls, and read owned snapshots through a short-lived mutex.
The worker owns capture-ring draining, audio send, received speech feeding,
route/grant invalidation, activity keepalives and device retry. It sleeps on a
stop-channel timeout rather than spinning. Audio callbacks and the existing
asynchronous device opener retain their original ownership. Worker creation
failure propagates through the existing App load result; shutdown closes the
transmit gate immediately and signals cleanup without joining a driver on UI.

Logical session identity now uses the App's RequestId separately from QUIC
transport identity. A new session resets local state. Replacing the transport
within the same session purges old grants/decoder/capture queues and activity
edges, while retaining the microphone/opener, held PTT and personal mutes. Native
volume, focus, disabled mode, preview, admin mute, retry and activation behavior
stay in the same state machine. The device factory is injected privately for
headless lifecycle tests; production defaults to VoiceChat::open.

Verification: `cargo check --locked --profile playtest -p bri-client --all-targets`;
`cargo test --locked --profile playtest -p bri-client --lib voice::` (nine cases);
`cargo clippy --locked --profile playtest -p bri-client --all-targets -- -D warnings`.
Final-source tests and Clippy pass. Initial type checks identified three unused
old getters after moving the public facade; those obsolete wrappers were removed
and their state assertions retained. No retry-only failures occurred. Logs use
`voice-pump-*.txt` in the established private release diagnostics folder.

The new actual QUIC test runs a loading participant with no UI update calls,
receives remote VAD, applies a personal mute/unmute and observes live route
revocation on the worker. It then enters gameplay through a verified arrival
ticket and proves the same pending opener, held PTT and personal mute survive.
The fixture supplies an ordinary NoDevice opener and a controlled pending opener;
it never opens hardware. The eight retained state tests cover prefs, indicators,
activation, personal mute, device changes, missing session and grant revocation.
This tests worker/control/lifetime behavior, not physical PCM capture or a real
microphone's sound continuity.

Next supported correction: gameplay reliable VoiceRoutes still travel through
`Incoming::Reliable` and Client::receive, whose bounded world-message queue can
backpressure during a frame stall. Arrival routes already update independently.
Separate critical voice control from that frame-consumed backlog, carrying the
explicit enabled/moderation/conversation metadata needed by loading UI. Then
install the accepted link before expensive preparation and bridge it to gameplay
without resetting this worker. The App still selects the gameplay link only;
this commit does not claim complete loading voice. Verify full capture/playout
and live privacy changes under loading pressure using device-free PCM and actual
QUIC before final acceptance. Maxwell's no-further-playtests instruction stands.
