# 2026-10-29 Ambience base mix raised six dB

Maxwell directly requested a 6 dB ambience increase through the release coordinator. Only the two authored indoor/outdoor beds change: apply amplitude 10**(6/20) after verifying the exact approved unity loop hash, before Ogg encoding. Preserve the 250 ms seam, 44.1 kHz stereo, 3,182,608 frames and EQ. Original inputs and unity audition loops remain unchanged. No normalization or limiting; reject clipping instead. Runtime profile volume remains 1. Storm, fridge, original waterfall/shore, tools and other gameplay sounds are unchanged.

The recipe records mix_gain_db and direct authority separately from original loop approval, and records pre-encode gain, unity loop hash and final encoded hashes. Candidate: artifacts/map-ambience/universal-candidate-r16. No changes to r15, running preview or Ableton review folder.

Low-cost validation while game was active: package validator empty; five package tests pass; py_compile and git diff --check pass. Actual decoded gain: indoor 6.002780 dB, outdoor 6.001018 dB (Vorbis quantization); decoded peaks -21.79345 and -16.60956 dBFS, finite/unclipped. Exact pre-encode gain verified within PCM24 quantization. Full duration/rate/channels retained. Rain/fridge encoded bytes and all sound definitions unchanged. Evidence: r16/gain-verification.json. No Cargo/GPU/build or interactive gameplay.

Initial generator invocation lacked local UI reference images; reran successfully using the existing combined preview content read-only. Coordinator owns content-only installation, preserved old package/manifest, startup checks and relaunch handoff. New in-game listening acceptance remains pending.
