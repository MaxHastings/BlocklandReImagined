# 2026-10-29 Map ambience visual audit and first audition

Lane `codex/map-ambience` reuses clean, merged `human-player-lists`, starting
at fetched origin/main `030b43a03`. Historical primary checkout and active
bot/destruction/release worktrees remain untouched. No new target or worktree.

Maxwell added ambience to v0.4.2 through release coordination. Inspected the
installed v0.4.1 package manifests, all 21 packaged map preview images, plus
existing Kitchen and Paradise captures. Inventory includes disabled-but-shipped
Arctic Challenge v3 and day/night variants; AvP is withdrawn, and private
cross-game maps do not ship. Exact visual inputs/hashes and candidate policy
are in `tools/map_ambience/maps.json`.

Prepared a concrete offline audition: ignored
`artifacts/map-ambience/audition/audition.html`, with 21 image/assessment cards,
10 actual source-linked CC0 recordings, quiet full loops, repeated join excerpts
and exact provenance. `tools/prepare_map_ambience_audition.py` reproduces it.
These are transparently labelled Freesound HQ **MP3-preview-derived** WAVs,
not lossless originals; seek accessible original-quality alternatives next.
The original-source-page snapshots and media stay outside Git. No subjective
listening was claimed from signal statistics.

Maxwell listened and said, "I like it as a base," then asked for occasional
bird/cricket/other details over time. This accepts the base direction, not
every recording, final level, loop or in-game spatial behavior. Next layer:
sparse environment-appropriate separate events, varied timing/location,
no immediate repetition, and quiet gaps through existing package sound calls.
Forest Dark's original crickets must not be doubled.

Confirmed original-content omission: Creek's imported report explicitly leaves
its three audio emitters Unadapted; Forest Dark's direct-file emitter is likewise
not exported. Read the exact pinned original ZIPs without modification; clip,
archive and resource hashes plus the supported next correction are in
`docs/audits/map-ambience.md`. Root granted bounded importer/helper/report paths.
The existing ambient seam forces reference distance to 25% of max range, so it
cannot preserve all four original emitters' authored falloff without a coordinated
extension; this limitation was reported before editing runtime schemas.

Maxwell also asked how arbitrary brick buildings affect ambience. Current
authored-region `SoundCarryModel` and reverb exist, but the client falls back to
clear hearing without authored regions and excludes `rule_ambient` handles from
its obstruction update. Automatic brick enclosure is not already implemented.
Recommended bounded geometry-derived shelter/openness, quieter/duller exterior
beds, local source obstruction, modest reverberation and smooth updates after
build/destruction. Root reserved the relevant client/audio helper paths and
requested exact intent/dataflow before schema changes; no runtime change yet.

Verification: ran the audition preparation command recorded in the handoff;
verified all 21 image references, 10 pinned input/output receipts, 20 decodable
48kHz stereo PCM WAV files, no clipping and every local report media link.
`python -m py_compile tools/prepare_map_ambience_audition.py` and
`git diff --check` pass. These are scoped artifact checks, not game startup,
converter compilation, full tests or listening acceptance. A candidate download
batch encountered HTTP403 for optional fluorescent buzz; that file was excluded,
and successful downloads/receipts were retained. No game launched or main pushed.

The concise dependency/acceptance owner handoff is
[`../audits/map-ambience.md`](../audits/map-ambience.md). Source-quality selection,
original-emitter conversion, sparse details, brick enclosure, actual shipping
recipe/policy, packaged startup and Maxwell's integrated listening remain open.
