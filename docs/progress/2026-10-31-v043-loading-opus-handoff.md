# Encoded audio through loading handoff

The real QUIC loading voice-policy fixture previously sent arbitrary bytes.
Its bidirectional exchanges now encode non-silent PCM with the native Opus
codec, assert the relayed bytes, decode the received frame and check for
non-silent samples. The checks cover loading-to-game and game-to-loading speech,
two loading participants, and both directions after the loading participant
enters; existing route-generation, moderation and isolation assertions remain.

The first run revealed an integration fixture mismatch: after the creator
leaves a minigame, the game now persists, so attempting to create another fails
with `You already own a mini-game`. The fixture now retains its first game ID
and rejoins it, exercising the intended persistent-ownership lifecycle. It
then passed. All 15 `voice_policy` cases passed with `cargo test --locked
--profile playtest -p bri-net --test voice_policy`; `cargo fmt -p bri-net` and
`cargo clippy --locked --profile playtest -p bri-net --test voice_policy -- -D
warnings` passed. The two harmless fixture warnings about missing weapon hand
nodes remain visible.

This tests codec and actual network relay through the handoff, not continuous
microphone capture, the independent client pump's PCM playout, or audio under
resource-load pressure. Those remain required separately before release.

Follow-up: the same real-QUIC fixture now feeds synthetic PCM into the native
push-to-talk `Encoder`, tags each packet with the host's capture scope, sends
the frames from the loading participant and again after entry, and feeds
received packets into the native `Mixer`. Both stages produce non-silent
rendered PCM. The exact integration case and `cargo clippy --locked --profile
playtest -p bri-net --test voice_policy -- -D warnings` passed. The mixer and
encoder are driven by the test, not the client's `VoiceClient` worker, and the
fixture does not apply map-download pressure or exercise hardware devices;
continuous live ownership across those transitions remains unproven.
