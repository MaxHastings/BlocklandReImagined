# 2026-10-29 Original-WAV ambience detail auditions

Maxwell likes the first preview as a base and requested previews of additional
bird/cricket/other layers. Prepared six concrete playable examples in ignored
`artifacts/map-ambience/audition/details/` and added them above the map cards in
the existing audition report: complete robin and blackbird calls, a7s cricket
phrase,18s grass movement,14s localized refrigerator tone, and a45s illustration
of quiet grass plus spatially separated calls at4,22and39s. This fixed schedule
demonstrates spacing; it is not a claim that the game scheduler is implemented.
No new cricket layer is selected over Forest Dark's authored crickets.

Unlike the initial continuous-bed preview inputs, these are derived directly
from openly downloadable original PCM WAVs by Joseph Sardin, from asset-specific
CC0 pages on LaSonotheque. Used the ordinary public WAV download forms and their
five-second wait; no login restriction was bypassed. Original media and page
snapshots remain outside Git. `tools/map_ambience/masters.json` pins exact input
hashes, source URLs, author, license, source resolution and quality labels for
eight original WAVs: room, fridge, grass, two birds, cricket, sea and an explicitly
studio-foley wind alternative. Master acquisition does not imply all eight are
chosen or accepted. A separate forest-edge original WAV was also obtained for
comparison; it is not silently added to the selected set.

`tools/prepare_map_ambience_details.py` applies a60Hz high-pass, stereo-preserving
or explicitly mono conversion, gentle25ms edge fades and conservative levels;
no pitch change or synthetic animal sounds. Exact excerpts, gains, rendered
durations, peak levels and output hashes are in `details-provenance.json`.

Commands:

```powershell
python tools/prepare_map_ambience_details.py --cache artifacts/map-ambience/inputs --output artifacts/map-ambience/audition/details
python tools/prepare_map_ambience_audition.py --cache artifacts/map-ambience/inputs --output artifacts/map-ambience/audition --content E:/Downloads/BlocklandReImagined-v0.4.1-windows/content
python -m py_compile tools/prepare_map_ambience_audition.py tools/prepare_map_ambience_details.py
```

Verified all six output hashes,48kHz decodability, finite samples, no clipping,
and every local media link in the updated HTML. These are artifact checks only;
Maxwell's response to these detail recordings and ordinary-gameplay mixing remain
pending. Playback is user-controlled; no visible game window or gameplay input
was automated. The existing base candidates remain available and transparently
labelled MP3-preview-derived; no claim of lossless quality is transferred to them.

Root has queued warm checks of runtime candidate `ce0b6e2c0`; no Cargo validation
is claimed here. Original emitter correction remains next. Related-family
OpenMBU disabled legacy AudioEmitter/AmbientAudioManager code was inspected as
**related-build evidence**, not original Blockland proof: it maps cone outer
volume to OpenAL outer gain and outsideAmbient to listener outside scaling.
Forest's outer gain1 is angularly neutral under those semantics; Creek's360-degree
inner cones are likewise neutral. Keep the evidence distinction and any remaining
source-fidelity uncertainty explicit in the converter handoff.
