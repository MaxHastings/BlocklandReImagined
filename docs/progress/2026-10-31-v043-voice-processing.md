# 2026-10-31 Native voice processing parameters for authorized packages

Maxwell clarified that modders should build radio/filter experiences; the engine
should expose useful seams rather than shipping finished effect presets. Extend
the existing read-only voice route owner with optional bounded linear gain,
high-pass and low-pass parameters. Defaults preserve the exact clean signal.
The server validates package results, routes them through existing private
generation-stamped playback tables, and reports invalid playback instead of
silently discarding malformed parameters. Local volume/mutes and the linked mix
limiter retain final control. No scripts execute on packets or PCM/audio callbacks.

The mono native processor owns stable first-order filters and sample-clock
smoothing. Coefficients are computed only on parameter/device-rate changes;
each source uses fixed scalar state, under the existing 32-source mixer cap.
The client explicitly maps the package/wire DTO to the audio configuration;
the simulation does not depend on the audio/Opus/device crate. Existing route
replacement/revocation drops the decoder, queue and processor together. The
wire addition is recorded in `v043-voice-processing.md`.

The first known-signal test exposed floating-point smoothing that could stall
slightly short of exact bypass (0.49999523 instead of 0.5). Retain exact bypass
acceptance: snap a sub-ULP stalled parameter to its target, rather than weakening
the assertion. Initial audio checks then pass 25. Expanded checks, real package
policy tests and changed-area Clippy are still pending below.

This increment is the filter/gain portion of the approved seam. Loading voice,
explicit world-speaker presentation, transmit lifecycle hooks and remaining
combined acceptance (including the planned local intelligibility preference)
are still separate work. No finished radio preset/add-on is
added or required, and no claim of human listening acceptance is made.

Final changed-area checks pass without retries after the smoothing correction:

- `cargo test --locked --profile playtest -p bri-audio --lib voice::`: 26 pass,
  including known-signal bands, exact dry/gain behavior across device rates,
  invalid native parameters, 31 filtered speakers through real Opus/mixing and
  immediate purge on mute.
- `cargo test --locked --profile playtest -p bri-net --test voice_policy`: 7 pass.
  A temporary, non-shipped authorized package returns default/custom/malformed
  parameters through real Rhai queries and authoritative route selection;
  existing capability, conflict, membership and real-network revocation cases
  remain. This is not an end-to-end hardware microphone test.
- `cargo test --locked --profile playtest -p bri-client --lib voice::tests`:
  8 pass, including local mute and generation/activity revocation.
- `cargo clippy --locked --profile playtest -p bri-audio -p bri-sim -p bri-net
  -p bri-client --all-targets -- -D warnings`: pass.

Private diagnostics are `voice-filter-{audio-integrated,policy,client,clippy}.txt`.
Final platform/history/protocol gates and release packages remain pending. The
loading/conversation increment must also challenge same-looking playback routes
across membership/team changes: playback equality alone is not evidence of the
same authorized conversation. Parameter-only live updates and their decoder
continuity should be assessed alongside that grant lifecycle, rather than
weakening revocation merely to retain DSP state.
