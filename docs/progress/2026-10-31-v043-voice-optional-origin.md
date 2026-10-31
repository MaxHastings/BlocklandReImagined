# Explicit nonpositional voice origin

The previous accepted-connection foundation correctly supplied loading routes but
the relay still required `Session::voice_origin`, and every downstream audio
packet required three coordinates. That makes bodyless authorized speech
impossible. Remove this structural dependency before adding arrival transport.

Downstream audio now carries a bounded placement tag: nonpositional frames have
no coordinates; spatial frames carry a finite authoritative origin. Route
generation remains host-stamped. Decoder bounds are checked against the selected
header, including payload size, unknown tags, truncated spatial coordinates and
nonfinite axes. The independent activity format is unchanged. A new protocol
change marker versions this alpha wire change; matching host/client is required.

The host derives coordinates only for spatial routes and rechecks live range.
A nonpositional route is independent of a body position. Client presentation
requires packet placement to agree with the authorized route. Native audio uses
`Playback::Nonpositional` or `Playback::Spatial { position, range }` instead of
parallel position/range values. Nonpositional mixing never computes world
distance or pan from an invented origin. Existing decoder, jitter, filters,
limiter, mute and bounded mixer ownership remain in place.

Checks so far: all-target type checking for audio/net/client passes, and all 26
`bri-audio --lib voice::` cases pass, including the revised real Opus/PCM case
that supplies no origin, moves the listener far away, verifies centered audibility
and preserves the existing playback-gain and revocation assertions. All six `bri-net --lib voice::` wire cases pass, covering both placement tags,
full-size payloads, generation identity, malformed headers and existing activity
and range contracts. Warning-free all-target Clippy for audio/net/client passes.
All eight `bri-client --lib voice::` cases pass. All five actual QUIC voice cases and twelve voice-policy cases pass with
`cargo test --locked --profile playtest -p bri-net --test voice --test voice_policy`.
They retain flood/range limits, 32 simultaneous speakers, loss/jitter/duplication
recovery, membership/privacy and conversation-generation checks. The real Global
case now asserts received frames omit coordinates and retain their current grant;
the proximity case still requires an authoritative position. Both
`cargo test --locked --profile playtest -p bri-net --test protocol_changes` cases
pass. No failures or retry-only passes occurred. These are scoped checks, not
platform CI, full release validation or human device acceptance.
Actual commands use `cargo test --locked --profile playtest -p bri-audio --lib
voice::`, `-p bri-net --lib voice::`, and `-p bri-client --lib voice::`.
Type checking and warning-free Clippy select audio/net/client with `--all-targets`.
Commands and outputs are in the established private diagnostics folder under
`voice-origin-*.txt`. No interactive device/gameplay test was run.

Still pending: arrival voice datagram readers and routing, accepted-to-body
transport replacement, background transport pumping during resource stalls,
loading conversation UI metadata and final integrated loading acceptance.
This increment does not claim loading voice is finished or a release is ready.
