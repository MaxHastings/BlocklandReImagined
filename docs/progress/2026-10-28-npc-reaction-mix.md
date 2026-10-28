# 2026-10-28 Audible NPC reactions during combat

Maxwell's Afghan playtest after PR #383 found the reactions very quiet.
The earlier preparation bounded each file's RMS/peak but did not calibrate
the complete positional gain chain against ordinary effects. The recipe's
0.8 profile gain and the script's 0.72 cue gain multiplied together, and a
38-unit radius scaled its full-volume distance down to 6.33 units. Stock pain,
gun and spear sounds normally use 10/60 units.

Decoded comparisons use the native audio pack and actual generated files, with
200 ms window RMS (90th percentile) as an activity measure, not a perceptual
loudness or player-acceptance claim. Across 118 reactions the old median active
level after authored gains was -22.76 dBFS nearby and -27.66 at 20 units.
Stock pain is -18.17 / -20.10, spear fire -11.75 / -13.69 and gunshot
-8.59 / -10.53 at those distances. Master/effects gains are common factors;
occlusion, stereo panning, pitch processing and simultaneous masking are not
included in these source-level comparisons.

The target file RMS is now 0.16 (about -15.9 dBFS), retaining the 0.88 peak
ceiling, 8x normalization limit and existing dynamics. Profile/cue gains are
unity and the positional radius is 60, preserving the ordinary 10/60 ratio.
The rebuilt median active level is -14.11 nearby / -16.05 at 20 units; weapon
fire remains louder. High-crest clips can remain below the RMS target. Other
effects, mixer settings, reaction selection, pitch identities, admission and
cooldown limits are unchanged.

All 118 files decode with distinct source hashes and maximum PCM peak
0.880005. Both generated Add-Ons pass the native checker. Quick/focused checks
pass, including all seven actual-policy cases and warning-free clippy. Private
before/after measurements and original generated packs are preserved in the
playtest handoff; the corrected pack is installed locally for the next restart.
Interactive listening acceptance remains Maxwell's; required integrated/PR
checks accompany the follow-up merge.
