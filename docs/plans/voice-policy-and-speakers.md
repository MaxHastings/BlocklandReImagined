# Voice policy and speaking display

Authorized by Maxwell on 2026-10-28 for the next release; implement, open PR, and merge after scoped checks and required platform CI. v0.4.0 remains immutable.

## Outcome
Keep the existing microphone, Opus, QUIC, bounded queues, personal/admin mute and audio device mechanisms. Move recipient and playback policy into ordinary enabled packages. No external service, codec rewrite, synthesized radio sounds, or cross-server voice.

## Implementation
- Add a bounded read-only `voice_routes(speaker, listeners)` package hook. It returns listener IDs, positional/global playback, range, short channel label and explicit priority. Multiple packages compose; highest priority wins per speaker/listener, so audio is delivered once. Equal-priority conflicts fail closed with diagnostics. No scripts run per audio packet.
- Resolve policy at a bounded cadence, caching results; continuously enforce host mutes and live distance. Refresh membership/state changes promptly. A private reliable route table and generation-stamped audio/activity prevent reordered packets restoring revoked speech. Clear buffered speech and activity when a route changes.
- Declare the default behavior in Voice Chat: outside minigames, global among outside players; inside, positional within the same minigame. Separate minigames and outside conversations remain isolated. Host/server and minigame settings override mode and distance through existing settings. Dead players in minigames speak only to dead members by default; living players are heard by dead members. A spectator listens from its camera, but cannot move its speaking origin by spectating another player.
- Supply an optional radio sample using existing server commands/state and held bindings, matching channels within the same conversation group; global radio playback takes priority over ordinary speech. Hold radio plus the ordinary voice activation control to transmit; the radio key never silently opens a microphone. Release/focus and stale held state must fail safe.
- Add a compact speaking list above chat, speaker glyph plus name, short release hold/fade, bounded rows and overflow count, with radio label. Respect eligibility and mutes, retain world indicators, expose the display through the existing HUD region mechanism.

- Increase received voice playback by 50% at the existing default slider position (Maxwell follow-up). Preserve microphone gain, mute behavior, and the existing linked mix limiter.

## Checks
Exercise default isolation, host overrides, radio composition/dedup, malformed/unauthorized hooks, mutes, death/spectating, membership/channel/reconnect/map changes, reordered packets and revoked buffered speech. Verify positional vs steady-volume audio without devices, HUD contents/overflow and customization. Run changed-area unit/integration tests and warning-free clippy, protocol/history checks and required platform CI. No interactive play automation; human microphone/device and subjective HUD acceptance remain Maxwell’s playtest. Record actual checks and any limits in the progress handoff.
