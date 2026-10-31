# 2026-10-31 Per-track rule music volume

Issue 412 asks for useful volume control on scripted music. The existing typed
`effects` operation now carries finite volume 0..=1, defaulting omitted Rhai
options to 1. The authoritative MusicPlay notice includes it, including retained
all-player music sent to later joiners. Its wire change has its own registered
protocol increment. No source assets or user slider preferences are modified.

Client playback multiplies the requested gain by the sound profile and retains
the existing Music/master controls and fade envelope. While tracing this chain,
confirmed that quiet non-looping music could hit effects MIN_GAIN culling;
its old fade workaround also began at 10% rather than silence. Explicit Music
tracks now bypass only that effects startup threshold, retaining enablement,
voice-count and resource budgets, and every fade starts at zero. This avoids a
burst and allows a quiet one-shot to fade normally. Source-backed effects
culling remains unchanged.

Checks added/extended: finite bounds, real script/session operation and later
joiners/default omission, native offline quiet one-shot start/mute/completion,
and client offline zero/quiet/quarter/full gain with looping and one-shot fades.
Formatting passes. All execution results, Clippy and final integration remain
pending; this entry is not a completed-acceptance claim.


Executed changed-behavior checks now pass: native offline audio runtime 27 cases;
finite operation bounds 1; actual script/session all-player, joiner, stop and
omitted-volume/default behavior 1; client offline looping/one-shot zero, quiet,
quarter/full gain, initial fade silence and local Music mute 1. Commands use
`cargo test --locked --profile playtest` with targets `-p bri-audio --test runtime`,
`-p bri-package-runtime --lib music_volume_is_finite_and_bounded`,
`-p bri-sim --test script_api rules_play_and_stop_music_for_everyone_and_one_player`,
and `-p bri-client --lib
rule_music_volume_and_fade_apply_to_looping_and_one_shot_tracks`. Private logs
are `diagnostics/music-{audio-runtime,op-bounds,script-session,client-offline}.txt`.
No hardware, window or interactive playtest was used. Clippy and final release
source/package/CI validation remain pending.


Warning-free all-target Clippy passes for client/net/sim/audio/package-runtime
with `--locked --profile playtest -- -D warnings`. The rebuilt headless probe
also links the new protocol/notice representation. The separate driver-route
regression remains open and is not counted as a passing bot suite here. Final
native platform/source/history/protocol/package gates remain release conditions.
