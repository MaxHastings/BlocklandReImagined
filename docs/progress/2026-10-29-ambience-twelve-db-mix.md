# 2026-10-29 Ambience beds another six dB

Maxwell directly requested another +6 in this ambience chat after the R16 +6 revision: R17 applies +12 dB total relative to the approved unity loops. Only user-bases.json mix authority/gain changes. Generator uses original PCM and verified approved unity seams, avoiding repeated lossy encoding. Raw input and unity loop hashes, 250 ms seam, 44.1 kHz stereo and 3,182,608 frames remain unchanged; no normalization/limiting/EQ changes.

Candidate artifacts/map-ambience/universal-candidate-r17: package validator empty; five package tests pass; git diff --check passes. Decoded gain versus R16: indoor 5.999865 dB, outdoor 6.001615 dB. Finite/unclipped decoded peaks -15.84270 and -10.71185 dBFS; exact pre-encode multiplier 10**(12/20) verified within PCM24 quantization. Rain/fridge media, sound definitions and server policy are byte-identical to R16. Evidence gain-verification.json records actual hashes.

No Cargo/GPU, game automation or existing preview/review folder writes. Coordinator owns isolated revision installation, preservation of previous mapping/media/manifest, startup/inventory verification and launcher delivery. Final in-game balance review remains Maxwell's.
