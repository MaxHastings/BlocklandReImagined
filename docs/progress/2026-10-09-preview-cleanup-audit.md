# 2026-10-09 Preview cleanup audit and carpet feedback

Max asked what the remaining PRs and PC folders contain. Read-only inventory:

- Four open PRs total: #31 Enhanced Sky, #33 historical release/audit notes,
  #34 the older combined integration including Gravity Gun, #35 the current
  stabilized preview excluding that integration. No PR was merged or closed.
- #34 is superseded for sky/shading playtesting, but its Gravity Gun changes
  must remain recoverable. After playtest, prepare separately named final sky
  and shading/AO PRs with the preview fixes; retire redundant preview PRs then.
- #33's note is dated history, predating v0.2.7.2; its body and current status
  need reconciliation rather than treating it as an outstanding game feature.
- Main checkout is at 3c8ed1f7, with eleven untracked progress notes belonging
  to other work (including character studies and jet animation investigation).
  Preserve these; no tracked source change in main was found.
- enhanced-sky has local-only commit 26087693 (sky probe) and untracked out/.
  soft-shading has local-only c816e534 (probe WIP) plus four modified files:
  lighting_probe.rs, ambient_occlusion.rs, scene.rs, modern_lighting.rs.
  Preserve/export local work before any worktree removal.
- Preview target is 26.48 GiB; soft-shading target 22.08 GiB; main target
  4.13 GiB. The dedicated gate directory is 54.13 GiB in total. Other folders
  are predominantly generated content, offscreen evidence and downloaded or
  extracted release copies, rather than additional Git worktrees.
- C: has about 1334.8 GiB free. No urgent disk shortage. A preview client is
  running from E:; the testing boundary remains Max's. No game input sent.
- clean_targets.py dry run with active/source worktrees and gate kept offers
  0.0 GiB. Regenerable preview output can be cleaned after current testing;
  keep main packaging output and preserve source/content/evidence.

Commands: git status --short, git worktree list --porcelain, git branch -vv,
git fetch origin --prune, gh pr list/view, Python recursive size inspection,
Get-PSDrive, process inventory, tools/clean_targets.py dry run. No deletion
or external PR mutation was performed during this audit.

New release-preparation issue: current #35 CI check job 113999687247 on run
37983469844 fails cargo fmt --all --check with Windows error 206 (command
filename/extension too long), rather than a formatting diff. The new client
test target crosses a known command-length boundary. Fix the formatter
invocation or test-module placement without dropping any checks before
landing. The private packaged runtime remains a4318d758.

Max supplied two Bedroom playtest images: the first has a noticeably cooler
carpet than the second. Code inspection confirms hemisphere() applies the
full peak-normalized sky tint to upward normals; it has no indoor sky
exposure factor. This is the current Soft Shading behavior, not AO colour.
The no-brightening check does not establish appropriate indoor colour.
Treat the strength of the indoor carpet tint as an open visual acceptance
issue; recommend reducing sky tint strength while preserving darkening and
original material identity. No runtime tuning was made from screenshots
alone, and no attached original-content images were added to Git.
