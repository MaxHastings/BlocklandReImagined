# 2026-10-10 v0.2.9 legacy Add-On porting handoff

Maxwell requested a writeup for Claude of friction, tooling/documentation gaps,
missing mechanisms and UX priorities from the P90/Rallypack work. Added
`docs/audits/v029-legacy-addon-porting-handoff.md` with a copyable brief,
prioritized deliverables, observed evidence, acceptance criteria and integration
pointers to the exact-release port branch. A copy is placed in the main
checkout for Claude to find; its evidence links point to this worktree.

Reviewed four committed port entries, the initial local audits, legacy import
docs and main's platform/seam guidance. The report distinguishes repaired
conversion/state defects from new engine seam candidates, preserves existing
distribution and basic bot evidence, and avoids claiming external internet
acceptance or advanced mechanics fidelity. Recommendations are a proposed
backlog, not implemented v0.2.9 features. No runtime/package changes, builds,
interactive playtests or external messages were needed for this documentation.
