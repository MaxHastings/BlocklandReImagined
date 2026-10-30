# 2026-10-29 Ambience beds eight dB total

Maxwell directly requested +8 dB total, replacing R17 +12 dB: lower only indoor/outdoor beds by 4 dB. R18 regenerates from the original approved unity loops with amplitude 10**(8/20), preserving raw input/unity loop hashes, 250 ms seam, 44.1 kHz stereo and 3,182,608 frames. No normalization, limiting, EQ or lossy reprocessing of prior revisions.

Candidate artifacts/map-ambience/universal-candidate-r18: package validator empty, five package tests pass, git diff --check passes. Decoded change versus R17: indoor -3.999555 dB, outdoor -4.000720 dB. Decoded PCM finite/unclipped, peaks -19.79632/-14.58689 dBFS. Exact pre-encode +8 dB verified within PCM24 quantization. Rain/fridge bytes, sound definitions and server companion unchanged. Actual hashes/measurements: gain-verification.json.

No Cargo/GPU/game automation or existing preview/review writes. Root owns separate plus8db installation, preserving prior revisions and settings, packaged startup/inventory checks and launcher handoff. User listening acceptance remains pending.
