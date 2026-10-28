# 2026-10-28 Accepted jet audio integration

Maxwell accepted the local audio playtest after final jets+3dB/landing+1dB
adjustments and requested merge to main. The longer listening/design history is
in [the playtest handoff](2026-10-27-blockhead-jet-playtest.md).

## Selected behavior

Native `sounds.jet` follows accepted jetting while alive and unmounted. One
positional PCM voice per actor plays its authored intro once, then loops from
0.33s with50ms attack/150ms cancellable release. Re-engagement preserves amplitude
and phase; releases/voice completion/session clear retire handles. Queue-full
release or profile-change commands retry instead of consuming their state edge.
The mechanism is generic; base Blockhead data opts in and Horse does not.

Final jet WAV SHA256:
`97635d15d97765efea7e2f2a93d8c98665fef4d06191f9bcb8ff98dfce52b8b7`
(peak-8.66dBFS). Final landing WAV:
`d2d0e79668fa032d57318fbc3efe21275db361f414f9249f3133e01d5d3ea1d5`
(peak-3.33dBFS). Landing is5dB above current main's level; walking is unchanged.
Both match the files accepted in `E:/Downloads/BlocklandReImagined-v0.3.7-windows`.

Known Freesound inputs retain author/source/license links and modification
credits; importer installs that credit file as a hashed native resource. The extra
attack sample remains identified as user-supplied/source unspecified, without a
fabricated license. Maxwell selected the current MP3-preview-derived export for base-game
inclusion; no new recording or mastering pass is part of this merge.

## Checks and handoff

- `python tools/check.py quick`: passed,487s, including698 client unit tests
  (78 generated-content cases reserved for the full gate).
- Initial focused Clippy rejected one nonminimal boolean; simplified it without
  changing behavior. Focused rerun passed (1,240s; 13 integration crates and 698 client unit tests). The final client Clippy check also passed and covers the queue
  retry safeguard as well as the earlier audio/asset changes.
- Mixer tests cover single intro, release/re-engagement, fractional44.1/48kHz
  wrap interpolation, render-block independence, invalid regions and virtual
  release. Adapter test covers frame reuse, moving release, queue pressure and
  ordinary5ms session-clear declick.
- Main-checkout generated content was backed up and refreshed only for owned
  jet/landing/credits and dependent presentation fingerprint; headless preflight
  passed. Physics preview uses independent content and remains untouched.
- Full integrated local gate and Windows PR CI are required before merge; their
  final results are recorded on the PR. Local logs are in the warm checkout's
  ignored `artifacts/jet-merge-checks` and the shared gate logs.

The playable folder keeps the accepted native audio; no Add-On is needed. Root
README and the physics branch/build are outside this change. Public main merging
does not turn the separate physics preview into a combined binary automatically.
