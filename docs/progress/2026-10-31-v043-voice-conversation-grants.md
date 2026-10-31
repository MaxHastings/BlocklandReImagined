# 2026-10-31 Voice grants follow conversation identity

The actual QUIC regression fails after a speaker joins a minigame: the route
remains allowed with an identical playback profile, and the receiver never gets
a fresh grant. Old server routing compared only audio presentation. A team-only
change was not even part of its immediate context refresh. Private
`voice-conversation-grants-red-2.txt` records the three-second deadline failure;
the first fixture extraction had a missing JSON macro import and unused inner
import, corrected before that real failure.

Grant reuse now requires both participants' unchanged transport generation,
minigame and team, the same authorized provider conversation, and the same audio
profile. Life changes still refresh policy; an unchanged allowed Global route
keeps its conversation/decoder generation. An unrelated speaker's unchanged route
survives another participant's transition. Cached route tables co-own their
host conversation metadata, so revocation, peer removal and map teardown do not
create a parallel orphan cache. Existing relay identity, origin/range, mute,
rate/freshness and activity checks remain in their owners.

An optional `conversation` string in a package's returned route lets a radio
change channels without changing its UI label or processing. Validate at most
96 UTF-8 bytes without controls, namespace by the actual provider (which scripts
cannot spoof), and retain that metadata on the host. `VoiceSelection` owns the
playback plus typed `VoiceConversation`; the existing playback projection delegates
to that single policy evaluation owner. `VoicePlayback` and wire route tables
carry no conversation key. No new packet/world/save format or protocol marker is
needed. An omitted/empty key keeps the native conversation, preserving agreement
between ordinary default providers. Explicit keys are provider-owned, so equal-
priority disagreement includes different provider conversations;
the optional existing radio sample now demonstrates the key. No preset or default
radio Add-On is added.

Checks completed so far:

- `cargo test --locked --profile playtest -p bri-net --test voice_policy`:
  11 pass on the completed source, including actual sender/listener minigame/team transitions with
  unchanged audio settings, preservation of an unrelated route, a package-state
  channel change through an ordinary command, bounded/control-free keys and
  provider identity absent from serialized playback. Existing Global/death,
  priority, authority/read-only and normal revocation cases remain passing.
- `cargo test --locked --profile playtest -p bri-net --test voice`: all five
  pass, including 32 concurrent talkers, gameplay commands, loss/jitter/duplication,
  range, disabled voice and malformed/flooded input. This is that bounded transport
  suite, not a full server-capacity performance campaign.
- Changed sim/net/client all-target Clippy passes with warnings denied. The
  shared client generation/revocation tests also pass all eight cases. Final
  changed sim/net/client all-target Clippy passes after failure-path corrections;
  formatting and `git diff --check` pass. No failing test is accepted by an
  unchanged retry.

The two-provider error regression first encountered the legitimate one-time
package-installation guard in its setup. Construct its full catalog before the
single install; keep the guard. It then reproduces a thrown private policy
falling through to public voice (`voice-policy-failure-red-2.txt`). Policy errors,
oversized route lists, invalid keys/playback and unavailable work budget now deny
that speaker's undecided refresh. Valid `[]` still abstains. The same case passes
for a thrown query, malformed gain and oversized conversation with ordinary
public voice present; the valid abstention control still permits public voice.
The final log is `voice-policy-failure-green.txt` (all 11), with final type and
Clippy receipts beside it. This strengthens the existing query owner rather
than adding a second permission system.

These checks do not complete loading voice. Arrivals still need the prospective
authorized audience and read-only policy view, chat delivery, nonpositional
playback, preserved connection-level capture/playout during resource loading and
actual arrival-to-game QUIC acceptance. Native arrival currently has identity
without body/minigame membership; preserve that distinction and team capacity.
World-speaker placement and transmit lifecycle integration remain open too.
