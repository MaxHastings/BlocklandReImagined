# 2026-10-28 Package voice policy and speaking HUD

Maxwell authorized the [full plan](../plans/voice-policy-and-speakers.md), implementation, PR and main merge for the next release. A separately human-authorized release coordinator now owns merge ordering/publication of the four active lanes for v0.4.1. v0.4.0 remains unchanged.

## Change

- Added an authorized read-only `voice_routes(speaker, listeners)` package hook. It runs outside the audio packet path, reuses ordinary script views/state/settings and work accounting, and composes explicit allow/deny routes by priority. Conflicting equal-priority policies deny that pair and report a problem. No secret `voice_chat` package-ID enablement remains.
- The default Voice Chat package selects global conversation among outside players and positional conversation within each minigame. Separate games/outside groups are isolated. Existing server/minigame settings expose scope and spatial range. Dead minigame members cannot speak to living members by default; dead members can hear living members.
- Private reliable route tables select playback and carry per-route generations. Downstream audio and activity carry those generations, so revocation clears queued audio/indicators and older datagrams cannot revive a route. Unchanged routes retain continuity. Membership/death/moderation changes refresh early; ordinary settings/channel and position refreshes are bounded to 200 ms. Audio delivery still checks live distance and host mute. Speech uses the body's eye, rather than borrowing a spectator target's position. Map changes invalidate route generations.
- An optional sample radio plus Controls companion demonstrates held activation, matching channels, global radio playback, and suppression of ordinary routes during radio transmission. It reuses existing commands/state/bindings and never independently opens a microphone. Radio commands permit release while dead; join/respawn clear stale transmit state.
- A compact microphone/name list above chat shows three speakers plus overflow, channel labels, and a 400 ms release fade. Names remain plain text. Existing world/F2 indicators remain. A new `voice` HUD region and `speakers` style support package positioning/styling or hiding.
- Maxwell's follow-up raises received live voice by 50% (1.5x). The slider, mute gates, microphone gain and linked mix limiter retain their roles. Recorded brick audio is unchanged.

The [modding handoff](../modding/voice.md) documents fields, bounded composition, sample controls and transition behavior. One protocol-change marker records the new reliable message and downstream generation fields; no manually edited VERSION or dependency changes.

## Evidence

- Warning-free all-targets clippy for `bri-audio,bri-net,bri-client,bri-ui,bri-package-runtime,bri-content,bri-sim,bri-package` passed; final post-gain confirmation is recorded in the PR.
- Full content/package/runtime/UI unit libraries: 546 passed, 11 pre-existing ignored cases. Includes live speaker rows, overflow, fade color and HUD hide/replace assertions; existing bounded synthetic UI render checks also passed.
- Voice-focused client/net/UI units passed, including route revocation clearing activity while preserving unchanged speakers, existing mute/settings controls and packet-shape checks.
- Audio voice checks passed after the gain change: default output is 1.5x the same decoded unity-gain reference below the limiter; global playback remains centered and audible beyond proximity range; revocation clears buffered speech/tail. Existing limiter/bounded mixer coverage remains. The initial raw-sine peak assertion was corrected to compare decoded/timed samples, because Opus, jitter buffering and the startup ramp do not reproduce that ideal peak.
- Real QUIC integration covers proximity, disabled voice, flood limits, loss/jitter and 32 concurrent speakers with ordinary gameplay commands. New tests cover default group isolation, radio override/denial/dedup, minigame settings/death, voice capability authorization, read-only mutation rejection, and network revocation/regrant with a new generation.
- A final review also separated the 32-source mixer limit from roster size: routing uses the existing server connection bound, with an added 33-player regression. Final roster/moderation and post-gain lint checks are handed to the coordinator alongside the source.

Initial native testing caught Rhai's f64-to-f32 decoding mismatch; returned bounded route maps now convert through JSON's typed deserializer before admission. Actual default/radio scripts run in the authoritative tests without fallback.

Commands and native logs: `artifacts/voice-clippy-final.log`, `voice-schema-ui-tests.log`, `voice-unit-final.log`, `voice-gain-final.log`, `voice-policy-final.log`, `voice-moderation.log` in this lane's worktree. These are scoped technical checks, not a full workspace gate or human microphone acceptance. Platform CI and integration merge are the release coordinator's final gate.

## Release handoff

Regenerate the canonical native `voice_chat` Add-On from source, including its new behaviour/script and manifest. It now infers server side; the old empty shared marker package does not enable voice under the new runtime. Keep optional radio samples out of default-enabled content unless explicitly selected. Rebuild protocol-matched binaries together; v0.4.0 peers are incompatible. Keep the accepted recorded +6 dB brick WAVs unchanged.

Human checks: two actual microphones, outside/global conversation, two isolated minigames, spatial fading, radio press/release/focus, spectator/death transitions, personal/admin mutes, and subjective HUD/gain comfort. No interactive gameplay was automated and no public release/build was dispatched by this lane.
