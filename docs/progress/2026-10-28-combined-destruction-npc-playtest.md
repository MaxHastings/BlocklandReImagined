# 2026-10-28 Combined destruction and NPC playtest

Maxwell requests latest main plus PR384 in the destruction playtest, using the
existing warm build, and explicitly authorizes merging384 after checks pass.
Retain the earlier authorization to merge the measured destruction improvement.

The private `plastic-destruction-playtest` worktree merged main `537fb8409`
(including PR383 navigation/reactions) and PR384 head `8bb5bcb1a` into the
destruction branch. Both merges were clean. The two-iteration cosmetic solver,
120Hz stepping and diagnostic spans are retained. No shared/main worktree or
original v20 source was edited.

The first destruction gate at `a26f9e097` ran all560 binaries and found one
persistent ignored-content-test failure: ItemData menu count66 versus69 loaded
items. Current content has three explicitly hidden items (dropped ammo, mortar,
scoped magnum). The item-choice mechanism already correctly excludes hidden
items; the old test wrongly expected every loaded item to appear. Replace that
count assumption with exact visible-ID set equality and a duplicate-choice
check. Let this ignored test honor BRI_CONTENT, as the gate does, so the same
actual content can be checked directly from a private worktree.

The corrected actual-content test passes against the main content (15.78s),
including its full lazy-map load checks. No assertion was skipped or failure
waived. Combined focused client checks pass in120s (format, clippy,732 units;
78 ignored). All seven actual NPC-policy integration cases pass. Correct the
remaining stale README radius38→60 to agree with PR384's existing code.

Refresh only the private worktree's independent content copy from current main:
42,491 files checked,29,467 copied. Rebuild all118 NPC recordings offline from
the private pinned cache with the merged384 recipe; the generated audio stays
ignored and local. The main content, global preferences and source recordings
are unchanged. Maxwell's existing Add-On choices already enable npc_popculture.
The generated companion and updated files must be present in the package;
pulling code alone would not deliver the intended louder reactions.

Next: warm paired preview.9 build, packaged startup/identity/hash verification,
and one full gate/CI of the integrated candidate. Check384's own final-head CI
as well before merging. The combined package is a private local playtest, not
publication of recordings. Human listening/feel acceptance and the remaining
render-side frame-cost question stay explicit.
