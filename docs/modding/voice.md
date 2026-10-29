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

`speaker` and `listeners` are connected human owner IDs. Read `player(id)`, minigames, package state and ordinary settings to choose policy. The hook cannot mutate state, emit effects, log, or perform operations, including indirectly through another package. Failed hooks contribute no routes and report a package problem. Use ordinary commands/hooks to update channel or inventory state. No voice packet invokes script.

A hook returns at most one entry per listener (bounded by the server connection limit); `hear` defaults true. Playback is spatial with server-owned range (1–2000 world units), or centered at steady volume. Labels are plain text, up to 48 bytes with no control characters. Priority is 0–255. Up to four policy providers compose: highest priority wins each speaker/listener pair, including explicit `hear:false` denials. Identical equal-priority results deduplicate; conflicting equal-priority results deny that pair and report a conflict. Personal and administrator mutes cannot be overridden.

Policy refreshes every 200 ms, with earlier refresh for membership, death, join/leave and moderation changes. Host settings and package channel/state changes apply within that refresh interval. This is bounded control latency, not an audio packet delay. Live distance is checked when audio is relayed. Reliable private route tables carry per-route generations; late audio/activity from revoked routes cannot revive them. Unchanged routes keep their audio buffers. Speech originates at the actor body; listening uses the controlled viewpoint.

The enabled `voice_chat` package declares outside/global and same-minigame/proximity defaults, server settings for outside scope/range, and minigame settings for mode/range. Dead minigame members do not speak to living members by default. No engine rule hardcodes these groups or the package ID. `voice` capability authorization is shown by the existing Add-On trust UI.

## Optional radio example

`packages/samples/sample-radio` plus its `sample-radio-controls` companion is an optional example, not default-enabled content. Install/enable both. Select a channel with `/radio_channel 1` (1–16), and bind **Transmit radio (with voice activation)** in Controls. Hold it while using ordinary push-to-talk or voice activation. The binding selects a radio route; it never independently enables a microphone. Radio has priority 10 over the default route; unmatched channels/groups are denied during transmission. On release, normal voice returns. Connection and respawn hooks clear the transmit state. Existing focus/input release handling and microphone focus gating still apply.

## Speaking HUD

A compact microphone/name list appears above chat, three names plus an overflow count, with a 400 ms release fade and a radio label. Only eligible, unmuted speakers appear. World name indicators and the F2 list remain available. Player names are plain text, not ML markup.

A HUD layout can claim `region: "voice"`. An empty claim hides the stock list; `speakers` draws native live rows with a custom rectangle, color and row count (1–6), using the layout's existing basis/placement:

```json
{"schema_version":1,"region":"voice","elements":[],
 "speakers":{"rect":[4,18,300,72],"color":[0.7,0.85,1,1],"rows":3}}
```

Received live voice has a 1.5x playback gain at the existing default volume setting; the slider and linked mix limiter still apply. Microphone gain is unchanged.

No external voice service or radio sound recordings are introduced. v0.4.0 clients require an update for the changed protocol. Device, microphone and subjective presentation acceptance remain human playtests.

The mixer limit of 32 simultaneous sources is independent of the player roster; a 33rd connected player does not disable voice routing. The current concurrent-talker transport check covers 32 players; this does not establish performance at the maximum theoretical server capacity.
