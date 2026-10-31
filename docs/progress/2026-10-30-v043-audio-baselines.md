# v0.4.3 audio preparation and preserved release baselines

Maxwell clarified that +6 dB tool swings and +4 dB ambience are starting points
for a natural, readable combined mix. Final balance must meet player expectations
alongside speech, impacts and combat, rather than optimize isolated numbers.

## Source preservation and processing

All three accepted swishes retain their pinned archive/member identities,
high-pass filter, variation and timing. The existing preparation recipe now uses
peak/RMS targets six dB higher and pins the resulting derivatives; the old
processed hashes remain explicit v0.4.2 evidence. Actual processing measures
+6.0000 dB RMS for each, with -6 dBFS peaks. The sword's empty Fire sound slot
uses the same profile, retaining Activate's `swordDrawSound` and original contact
policy. The tool augmentation regression checks preservation and repeatability.

The old raw ambience WAV caches were absent from the available source/release
workspaces and evidence. Maxwell confirmed he no longer has those exports and
explicitly authorized the existing OGGs. Preserved all four shipping ambience
OGGs under `E:/Downloads/BlocklandReImagined-Release-Evidence/v043-audio/v042-ambience`,
and copied verified working inputs to ignored `artifacts/map-ambience/inputs`.
`tools/map_ambience/user-bases.json` pins source revision, paths, hashes and
the previously baked gain. Original recording/author/approval lineage stays
separate from these derived release inputs.

The generator reads those immutable loops, checks their hashes and applies the
requested gain relative to the release. It does not reapply the loop seam or
treat decoded lossy data as the original WAV. Rain and the localized fridge loop
are copied byte-for-byte. The two adjusted bases receive one high-quality q10
Vorbis encode from the fixed baseline on every regeneration. Subsequent tuning
never consumes a previously adjusted derivative.

A PCM24 shipping attempt failed the native checker's existing 8 MiB encoded
track bound. Kept that bound rather than expanding audio resource limits. The
first q8 encode passed loading but measured +3.959 dB indoors, outside the
offline comparison's 0.025 dB tolerance. q10 improves preservation while staying
well inside the same byte bound. No assertion or resource bound was relaxed.

Final decoded measurements relative to the fixed v0.4.2 OGGs:

| Clip | RMS change | Encoded bytes | Peak dBFS |
| --- | ---: | ---: | ---: |
| Indoor | +4.015485 dB | 3,950,531 | -11.89 |
| Outdoor | +4.020979 dB | 4,172,834 | -6.70 |
| Rain | 0 dB, identical bytes | 3,022,421 | -11.57 |
| Fridge | 0 dB, identical bytes | 361,317 | -28.15 |

Both base loops retain 3,182,608 frames, stereo and 44.1 kHz. Private JSON
comparisons record actual hashes and measurements. Isolated headroom and RMS
are not combined limiter, true-peak or in-game mix acceptance.

## Checks and current state

`test_tool_swishes` 2, `test_map_ambience_package` 6, generated-Add-On tests 2
and source-approval tests 2 pass. The package test includes rejection of a
changed release-baseline cache. Existing recipe hashing covers the baseline metadata;
generated-package verification reports no problems. Both actual native ambience
packages pass the installed v0.4.2 Add-On checker after the encoding correction.
The new build's packaged startup is still pending.

Cloned release content into this worktree only. Applied the swish policy to that
clone and ran the real item-presentation, weapon-debris and weapon-effects
generation consumers against the changed weapon hash. The recovered source core
is a read-only copy from the existing private research, SHA256
`a720f7a188a615276befa34409eda68585ec9abbc629b7a8107f37dcf7b33a9b`.
New consumer folders have `-v043` suffixes and private package mappings; source
installations and the installed release remain unchanged. Final freshness,
packaged startup and mixed listening remain open. No original media enters Git.
