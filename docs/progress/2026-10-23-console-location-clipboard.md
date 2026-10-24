# 2026-10-23 Console coordinates and clipboard diagnostics

Maxwell requests a diagnostic playtest before more Interdiction lighting work,
so he can provide exact locations of invisible collision. Add the generic
`where` console command: map/build identity, owner/tick, native Y-up feet,
grounded/crouched state and rendered camera eye/yaw/pitch. Output is logged and
copied to the system clipboard; clipboard failure leaves the printed report.

The native window host now handles console Ctrl/Command+V and C. Paste travels
through ordinary character input and replay recording, normalizes line breaks
to spaces, respects the existing edit limit, and never submits. Copy takes the
entry or the bounded console log when the entry is empty. OS clipboard access
stays outside the UI. Text edits support visibly selected Ctrl/Command+A,
replacement, deletion and arrow collapse. Console routing still requires Enter.

AA's building HUD claim also swallowed existing F8/F7 commands because the
generic UI classified admin camera controls as building actions. Remove that
classification; keep server authorization and explicit package camera policies.
The retained control-profile test fails with no F8 action before the correction
and all eight control-profile cases pass afterward, while brick keys still stay
hidden. This fixes an engine UI boundary, not AA gameplay policy.

Quick and focused checks pass for the console/UI changes. Eight console UI
cases include replacement, copy and no automatic execution; the native input
helper covers line breaks, control characters, text bounds and accented input.
The complete final focused tier, gate, Windows CI and private package receipts
must precede delivery. OS clipboard and interactive camera acceptance remain
Maxwell's playtest; automated input routing alone does not establish those.

Lighting work is preserved on `codex/interdiction-region-lighting-20261023`
at `80a363e68`; it is intentionally separate from this diagnostic delivery.
The broader collision/lighting goal remains open. The first jail-route failures
were a closed door and an open-loop waypoint driver, not a proven map defect.
With normal Use and corrected steering the route passes on unchanged production
collision. Direct static collision/visible-geometry sections and original saved
mover-lighting evidence remain in the private jail-report archive. No collision
fix or source-game appearance acceptance is claimed by this diagnostic build.
