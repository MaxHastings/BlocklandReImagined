# v0.3.1 wrap-up scope and Linux reports

Maxwell explicitly lowered Add-On priority and asked Halo, AA and GMod to finish
their current work, push it into the private Add-On repository and park. Root
sent that instruction to all three and changed the existing heartbeat so it
cannot automatically restart their fidelity roadmaps. Shared engine keeps
GPT-6.1 Sol Medium and finishes its current bounded animation seam; later
fitting, curve, projectile and Add-On work is deferred unless newly requested.

The previous requirement to finish every agreed game Add-On before v0.3.1 is
superseded. Source research and incomplete branches remain preserved rather
than described as completed fidelity. Root reviews ready private checkpoints
and retains unfinished Halo PR34 separately. Both delivered previews stay
unchanged. No publication before Maxwell's final candidate test and approval.

## Reported release bugs

ZaRR reports on Linux v0.3.0:

1. After picking up an item in the tutorial, mouse-wheel selection reaches older
   items but cannot select the newly acquired item. Dropping an item restores
   selection. Investigate the authoritative inventory-to-HUD update and selection
   acknowledgment path; simple static inventory cycling alone is not a sufficient
   reproduction.
2. Paint selection skips a swatch or row on a wheel movement. Investigate event
   normalization, high-resolution wheel accumulation and input routing. UI scale
   was mentioned; its causal role has not yet been established.
3. The attached 803 by530 tutorial completion screenshot visibly clips the final
   instruction below the statistics. Investigate scaled center-print layout and
   content height independently of wheel input. The screenshot does not establish
   a missing tutorial goal;17/18 is recorded but needs its own evidence before
   calling it a separate bug.

No fixes or Linux reproduction are claimed yet. All interactive tests remain
Maxwell's. Use headless input/replication tests and bounded offscreen rendering,
then request Linux verification for behavior that cannot be exercised on Windows.

## Root work isolation

Root created `codex/v031-linux-wheel-fixes` in the required sibling worktree
directory from frozen sampler candidate f554ebe817bbbeb08ed21e35a4c029f6d8b9df2e.
No eligible clean, merged idle checkout was identified (the inspected v030
checkpoint is not an ancestor of main). This leaves the primary sampler gate
and all worker changes intact. No cargo build or game window was started here.
Sampler PR175 and full gate session10705 remain independent pending checks.
