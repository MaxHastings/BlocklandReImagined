# 2026-10-31 Voice capture conversation scope

Extend the existing voice control and capture owners to prevent queued speech
from being reassigned after a conversation change. Host-issued source epochs use
policy-eligible outgoing conversations before proximity distance filtering;
provider keys and authoritative participant transport/game/team changes rotate
the grant. An empty audience or server mute permits no transmission. Epochs are
independent of inbound listener routes and travel on upstream audio/activity.
The host checks them at admission to the relay; the native sender also drops
stale queued frames. The wire change has its own protocol marker.

The capture encoder tags frames before queueing, clears partial PCM and restarts
Opus/FEC state at a scope change. A delayed device buffer is not relabeled with
the latest scope: a capture-clock boundary conservatively requires a whole
buffer beyond the callback that observed the change, resets the downsampler,
and rejects reversed timestamps. Scope zero still feeds the local input meter
but cannot transmit. No audio-callback lock, packet script, new effect algorithm,
real audio device, or interactive game is introduced by validation.

Maxwell reiterated that voice customization must not run away. The detailed plan
now explicitly caps this release at the already implemented bounded gain and
high/low-pass parameters plus necessary routing/lifecycle work. Finished radio
presets, extra DSP, plug-in hosts and general effect/profile editors remain out.

Current evidence: net-only and audio/net/client all-target type checks pass.
Extracting the shared two-player control fixture initially omitted its closing
parenthesis; the syntax error was corrected before runtime tests. Device-free
voice checks now pass both before and after tightening stale-activity/closed-link
handling: 29 audio, 13 client and ten net cases with `cargo test --locked --profile
playtest -p bri-audio -p bri-net -p bri-client --lib voice`. The repeat was required
by the new activity-scope guard, not a retry of a failing test. New cases challenge
partial PCM/codec reset, delayed capture timestamps, meter preservation, and actual
raw QUIC stale-epoch audio/activity after membership regrant. A shared two-player
fixture replaces duplicate control-server setup.

`cargo test --locked --profile playtest -p bri-net --test voice --test voice_policy
--test chat_while_joining --test protocol_changes` passes 27 cases. The existing
package-channel case additionally proves that the source epoch changes even
while the sender's incoming route table stays identical. The 32-speaker actual
QUIC/Opus case and existing map/moderation/loading cases remain green.
`cargo clippy --locked --profile playtest -p bri-audio -p bri-net -p bri-client
--all-targets -- -D warnings` and `git diff --check` pass. These are 79 distinct
scoped cases; the earlier 52-case run is not counted twice. No workspace-wide,
GPU, hardware or final release pass is claimed. Private logs are
`voice-source-{lib-tests,lib-final,network,clippy}.txt` in the planning diagnostics
directory.
Combined continuity, remaining listed seams, full release checks and the broader
v0.4.3 scope remain open.
