# 2026-10-16 v0.3.2 fitting ownership checkpoint

Isolated branch `codex/v032-engine-seams`, immutable base
`3117e8c36c460f20bfc571700108890a9d4b7ad4`; initial status clean. Main release
and shared Stage1 checkouts remained read-only. No dependencies, agents,
production changes, game windows, original content changes or publication.

Read AGENTS, STATUS, alpha/platform/testing contracts, historical progress and
recent root/layer/root-shift records; inspected mounted-pose-authority's concrete
Stage B/C plan and the shared owner's authoritative-animation-completion design
and Stage1 progress read-only. Traced Avatar fits/fit_hands/finish_pose and socket
methods, existing body_pose layer planner and hold primitives. Read private
passenger, AA actor/reload and vehicle-material handoffs through pinned git show.
The private working checkout's current main differs from the supplied evidence
pin; all cited evidence uses explicit commits, not that moving branch.

New architecture note proposes first production paths limited to content's
existing body_pose.rs and client avatar.rs, with source-free characterization
tests in their unit modules. It specifies BodyFits/build_body_fits and
BodyFittingInput/apply_body_fitting, preserving cache/role identity, authored
hands, support liveness versus final key, right-before-left world seat reach and
completed inverse root frame. Socket extraction follows as a separate atom.

Pending immutable Stage1 is not copied or duplicated. Completion hooks,
replication/client channel15, original timing, authored prone, host pose/eye/fire
and contact/material/frame work remain distinct follow-up reservations. Root
must assign exact production paths after reviewing this proposal. v0.3.1 release
integration is unaffected; Halo/AA active, GMod parked under latest assignment.

Commands: git status/rev-parse; bounded Get-Content and rg production audit;
private git ls-tree/show at pinned revisions. Initial rg with Windows wildcard
paths failed (invalid path syntax); corrected searches name concrete files.
Some combined reads exceeded output budget; relevant functions and contracts
were reread in bounded slices. No runtime test result is claimed for this
documentation-only checkpoint.

Validation: `git diff --check` passes. `python tools/check.py quick --base HEAD`
and `python tools/check.py focused --base HEAD` both report no crate changed,
nothing to run. This is a documentation-only tier result, not runtime acceptance.
