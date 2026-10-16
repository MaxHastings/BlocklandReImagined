# Tutorial completion print clipping

Root reproduced the reported multiline tutorial completion layout in the real
headless UI on Linux settings, first with the source-free font fixture and then
the generated vanilla UI pack. Before the correction, the clipping parent's
90-unit height could not contain the measured 144-unit print. The regression
failed before the production edit (target-print-baseline.log).

Play now measures the actual multiline text at its current logical width and
grows both the text control and its clipping parent, retaining the authored
minimum height and keeping the print within the available logical canvas when
it fits. This uses existing UI layout and does not change font or UI scaling.

Evidence: actual pack and synthetic regressions pass at 803x530 scales1/1.5 and
1280x960 scale2, including long-short-long replacement, repeated updates, resize
and expiration. Explicit generated-content run passes2/0/0. Scoped quick passes
226/0/11; focused exits0 in78s (UI tests, all-target Clippy, formatting and the
explicit synthetic integration target). Commands/results are retained in
target-print-{baseline,corrected,content,expanded,quick,focused}.log.

This is measured layout evidence, not interactive gameplay or screenshot
acceptance. No wheel fix or full integrated candidate gate is claimed here.
