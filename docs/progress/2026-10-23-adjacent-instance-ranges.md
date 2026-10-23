# Preserve sorted transparency while joining adjacent instance ranges

Interdiction's optimized underground captures submit roughly 2,457 translucent
draws. Faded instances are individually sorted even when a run already names
contiguous ascending ranges in one buffer. After the complete global ordering,
the candidate joins only adjacent draws with the exact same scene, batch,
instance buffer and blend, where the previous end equals the next start. It
neither changes the global order nor spans intervening objects or reversed
ranges. Existing pooled brick multi-draws, shadows and geometry are unchanged.

The retained GPU regression fails before correction (two draws instead of one)
and passes afterward. Independently CPU-transformed geometry provides its
pixel oracle. A differently colored object between the instances remains
between them (three draws); an opposite eye order remains two draws. Atomic
instance-update/fade/normal checks remain. All 18 ordinary persistent-scene
render checks pass (three ignored cases remain for the integrated gate), and
quick passes 83 render unit checks in 10s. Focused checks and actual-map draw
reduction are still being measured. No FPS improvement or complete map-culling
fix is claimed. Static map spatial subdivision remains the next dependency.

Ignored evidence: `.research/pipeline-integration/adjacent-instances-*` in the
engine worktree and `capture-adjacent-ranges-first` in the private integration
scratch root. The existing delivered preview is unchanged.

Focused passes in 566s (ten dependent crates, client/render units, clippy and
format). The actual wall-view work comparison falls from 3,226 to 3,050 draws
(5.5%), including 2,457 to 2,425 translucent draws; the matched 1,958,370-triangle
load remains. The after frame was inspected. This is useful limited work
reduction, not the main map-performance solution. Continue static world range
culling before gating/merging the combined performance increment.
