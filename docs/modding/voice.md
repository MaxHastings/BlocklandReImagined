# Live voice policy

The engine captures, encodes, relays and mixes voice. Enabled host packages with the `voice` capability and behaviour `on_voice: true` supply the read-only function:

```rhai
fn voice_routes(speaker, listeners) {
    let out = [];
    for id in listeners {
        if id != speaker {
            out.push(#{listener:id, hear:true, spatial:false, range:48.0, label:"", priority:0});
        }
    }
    out
}
```

`speaker` and `listeners` are connected human owner IDs. Read `connection(id)`, minigames, package state and ordinary settings to choose policy. `player(id)` remains a body-only view. The hook cannot mutate state, emit effects, log, or perform operations, including indirectly through another package. Failed hooks contribute no routes and report a package problem. Use ordinary commands/hooks to update channel or inventory state. No voice packet invokes script.

A hook returns at most one entry per listener (bounded by the server connection limit); `hear` defaults true. Playback is spatial with server-owned range (1–2000 world units), or centered at steady volume. Labels are plain text, up to 48 bytes with no control characters. Priority is 0–255. Up to four policy providers compose: highest priority wins each speaker/listener pair, including explicit `hear:false` denials. Identical equal-priority results deduplicate; conflicting equal-priority results deny that pair and report a conflict. Personal and administrator mutes cannot be overridden.

Policy refreshes every 200 ms, with earlier refresh for membership, death, join/leave and moderation changes. Host settings and package channel/state changes apply within that refresh interval. This is bounded control latency, not an audio packet delay. Live distance is checked when audio is relayed. Reliable private route tables carry per-route generations; late audio/activity from revoked routes cannot revive them. Unchanged routes keep their audio buffers. Spatial speech originates at the actor body; listening uses the controlled viewpoint. Nonpositional packets carry no coordinates and native playback requires no synthetic origin.

The enabled `voice_chat` package declares outside/global and same-minigame/proximity defaults, server settings for outside scope/range, and minigame settings for mode/range. Global lets living and dead members talk together. Proximity and Team retain their dead-to-living restrictions. No engine rule hardcodes these groups or the package ID. `voice` capability authorization is shown by the existing Add-On trust UI.

## Accepted connection view

`connection(owner)` is a read-only human connection view, or `()` for an unknown,
offline or bot owner. It returns `id`, `name`, `loading`, `minigame`, `team`,
`alive` and `x`/`y`/`z`. A loading connection has no body: `team`, `alive` and all
position coordinates are `()`. Check `loading` before using those fields.
`minigame` is its authorized pending destination, not membership or permission
to perform gameplay commands. Actual players retain their current membership.

Pending destination prefers a valid retained membership proven by authenticated
owner reuse, then the host's automatic server/default game, otherwise lobby
(`()`). It follows changes to those grants. It grants no team slot, invented
position or previous private chat history; map replacement does not carry old
game IDs into a new world. Explicit membership removal revokes retention.
Eligible pending connections receive subsequent scoped game notices and kill
feeds. The host-selected default uses the existing automatic-entry authority;
an unrelated invite-only game supplies no grant.

The stock policy uses nonpositional routes when either participant is loading.
It does not infer a loading team or dead-player channel. The optional radio
example denies private transmissions to loading listeners with no established
channel. Accepted arrivals now expose the same bounded VoiceLink transport as gameplay,
including reliable route updates and audio/activity datagrams. This does **not**
establish continuous client loading audio: background capture/send/playout and
stable audio ownership across transport replacement remain pending.

## Native processing parameters

An eligible route may include `effects`. Omitted fields mean clean speech:

```rhai
#{ listener: id, hear: true, spatial: false, range: 48.0,
   label: "Authored channel", priority: 10,
   effects: #{ gain: 0.8, high_pass_hz: 300.0, low_pass_hz: 3000.0 } }
```

| Parameter | Meaning and bounds |
|---|---|
| `gain` | Linear gain, 0–2; default 1. Applied before the listener's voice volume and the native mix limiter. |
| `high_pass_hz` | First-order high-pass cutoff, 40–4000 Hz; 0 bypasses it (default). |
| `low_pass_hz` | First-order low-pass cutoff, 100–7500 Hz; 0 bypasses it (default). |

When both filters are enabled, the high-pass cutoff must be below the low-pass
cutoff. All values must be finite. Unknown fields and invalid values report a
package problem and deny that speaker's refresh, including fallback to a lower
priority public provider. A successful empty route list explicitly abstains. Combining the filters narrows the
frequency band; these numbers illustrate composition, rather than a required
radio sound. Packages own tuning, channel policy and presentation.

The native mono processor smooths gain/filter changes on the sample clock,
adapts cutoffs to the output device's Nyquist limit and allocates no per-sample
objects. Clean defaults bypass processing exactly. The existing 32-source mixer,
queues and limiter remain bounded. A route change receives a new generation and
discards the previous decoder, queued speech and filter state together. A mute
or revocation clears them immediately; changing effects never grants eligibility
or bypasses a listener's mute/volume. No package code receives microphone PCM,
runs on an audio callback, or loads arbitrary native plug-ins.

## Conversation identity

A route may return `conversation: "radio/3"`, a stable key of at most 96 UTF-8
bytes without control characters. The host namespaces it by the authorized
provider package. An omitted or empty key selects the native conversation,
so agreeing default providers can still compose. It is host-only metadata, separate from the displayed `label`
and audio effects. Keep it stable during a conversation; change it when a radio
channel or another logical audience changes. Per-frame keys cause decoder resets.
The optional radio example uses `"radio/" + channel`.

A changed key retires the old grant even when its label, range and filters are
identical. The host also binds grants to both participants' current transport,
minigame and team. Unrelated unchanged routes keep their generation; a life-state
change refreshes policy and preserves an unchanged allowed Global conversation.
Revocation/new generation uses the existing client decoder, speech and activity
reset path. A key does not grant eligibility, join a game, select arbitrary audio
code or replace personal/admin mute rules. Equal-priority providers selecting
different conversations conflict and deny that route; use explicit priorities.
A policy error, malformed selection or exhausted policy budget denies that
speaker's refresh; it cannot fall back to a more permissive public route. A
successful empty list remains a valid abstention.
No new audio packet, world or saved-build field is added by these host keys.

## Optional radio example

`packages/samples/sample-radio` plus its `sample-radio-controls` companion is an optional example, not default-enabled content. Install/enable both. Select a channel with `/radio_channel 1` (1–16), and bind **Transmit radio (with voice activation)** in Controls. Hold it while using ordinary push-to-talk or voice activation. The binding selects a radio route; it never independently enables a microphone. Radio has priority 10 over the default route; unmatched channels/groups are denied during transmission. On release, normal voice returns. Connection and respawn hooks clear the transmit state. Existing focus/input release handling and microphone focus gating still apply.

## Speaking HUD

A compact microphone/name list flows horizontally above chat and wraps at its measured width, with an overflow count, a 400 ms release fade and a channel label. Only eligible, unmuted speakers appear. World name indicators and the F2 list remain available. Player names are plain text, not ML markup.

A HUD layout can claim `region: "voice"`. An empty claim hides the stock list; `speakers` draws native live rows with a custom rectangle, color and row count (1–6), using the layout's existing basis/placement:

```json
{"schema_version":1,"region":"voice","elements":[],
 "speakers":{"rect":[4,18,300,72],"color":[0.7,0.85,1,1],"rows":3}}
```

Received live voice has a 1.5x playback gain at the existing default volume setting; the slider and linked mix limiter still apply. Microphone gain is unchanged.

No external voice service or radio sound recordings are introduced. Voice routing metadata is protocol-versioned; clients and hosts must run matching versions. Device, microphone and subjective presentation acceptance remain human playtests.

The mixer limit of 32 simultaneous sources is independent of the player roster; a 33rd connected player does not disable voice routing. The current concurrent-talker transport check covers 32 players; this does not establish performance at the maximum theoretical server capacity.
