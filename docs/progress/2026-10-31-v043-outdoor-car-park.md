# 2026-10-31 Outdoor base replacement

Maxwell explicitly chose `E:/Downloads/350267__simonjeffery13__outside-in-the-car-park.wav`
for the outdoor base, delegated level balance and required an OGG game copy. This
supersedes the earlier decision to keep the previous outdoor recording. The
original WAV stays unchanged; source media remains outside Git. Its SHA256 is
`3c251ff8d582420f4598ff1bce6889b140959bde4a479cb26f416b18a4ae82a9`.
The official [Freesound page](https://freesound.org/people/simonjeffery13/sounds/350267/)
identifies simonjeffery13 and CC0; retained primary-page HTML privately after the
web tool could not open/index it. The source/author/license and exact user request
are recorded in the existing approval registry. Standard preparation is authorized
by that request and the prior playtest waiver; no new human hearing acceptance is
claimed or required.

The raw stereo PCM16, 48 kHz recording is 70.880 seconds and 13,622,416 bytes.
The existing 250 ms linear loop seam retains 3,390,240 stereo frames at 48 kHz
(70.630 seconds). Apply fixed +18.21 dB, with unchanged EQ/dynamics. FFmpeg's
loudnorm filter measures inputs only; its normalized output is discarded. No
compression, limiting or extra +4 dB is applied. Matching the prior v0.4.3 outdoor
base gives the following decoded game copy:

| Measure | Result |
| --- | --- |
| Codec | Ogg Vorbis q6, encoded once from the supplied lossless source |
| Integrated loudness | -25.19 LUFS (prior outdoor -25.06; difference -0.13 LU) |
| RMS | -27.111 dBFS |
| True peak | -9.31 dBTP |
| Encoded size | 1,550,859 bytes, 88.6% smaller than the source WAV |
| SHA256 | `f4d0f075950595f7c76a81a36186121e69211cc0a7de8b45bed6317bcf909f29` |

The decoded loop-boundary step is 0.00103 versus a within-clip 99.9-percentile
sample step of 0.01950. This is an offline continuity check, not human listening
acceptance. All existing nonstorm outdoor routes keep the stable
`map_ambience:outdoor-base` profile; indoor, rain and fridge OGGs remain byte-for-byte
identical to their previous hashes. The generator now explicitly selects a
supplied recording instead of accidentally reading the old same-named release
baseline. Its existing identity test challenges that distinction; unknown input
kinds fail closed. Historical v0.4.2 baseline metadata remains preserved.

Generated the actual package in the integration worktree's
`content/addons/map_ambience`, with fresh recipe/source receipts and credits.
Also copied the ready OGG to
`E:/Downloads/BlocklandReImagined-Ambience-Edits/outdoor-base-car-park-v043.ogg`.
Source and before/after measurements are under private release evidence
`v043-audio/outdoor-car-park/`; diagnostic logs use `outdoor-replacement-*`.

Validation: `python -m unittest discover -s tools -p 'test_map_ambience*.py'`
passes 12; `test_generated_addons.py` passes two. Actual generated-package
verification returns no problems. `target/playtest/bri-addon-check.exe
content/addons/map_ambience --json` returns ok with no diagnostics. Native
encoded-track bounds remain unchanged, and no audio device or visible game is
opened. `git diff --check` passes. The WAV's hash is rechecked after generation.
Combined final-candidate/package/release gates remain part of the active goal;
this correction does not publish v0.4.3 or claim new in-game listening acceptance.
