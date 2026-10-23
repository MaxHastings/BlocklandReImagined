# 2026-10-23 Integrated release candidate

Maxwell subsequently says to ignore PRs and use integrator judgement. The goal
is a verified useful release, not an empty PR queue. Preserve unfinished
experiments without treating their existence as a release requirement.

Local candidate after 0d7457fcd integrates reviewed changes from #207 (fresh
Player Throwing state, sound-only explosion references and clearer bot exclusion
messages), #289 (bounded stall/neighbor diagnostics), #290 (bounded casing origin
and uploaded extent diagnostics), and #287 (the paired loose-object release
hook). The indexed-path overlap in #207 retained the new containment regression
and package-root comment. These integrations are local, not yet engine main.
Night vision #284 and climbing #286 remain unfinished, conflicting proposals;
they are preserved as future AA work rather than silently shipping unqualified
physics/presentation. The full AA execution plan remains in force.

The Use-release review found a concrete defect: dispatch used the consuming
input helper, so a callback returning true prevented later subscribers from
finishing. New ordinary Session/rule tests fail on that implementation and pass
with non-consuming release dispatch. They cover an unmatched release, duplicate
release, removal of the original target, identical retained target for two
subscribers and release after death. No interactive game is run. The release
hook documentation now describes its actual lifecycle rather than asserting
unverified independent trigger semantics.

Integrated quick checks pass (403 s). The focused run includes strict affected
all-target clippy (passes, 108 s) and 20 dependent crates; the complete focused
run passes in 1424 s. This is not a final gate receipt. Documentation checks pass for
69 maintained documents and six generated platform guide layouts; the Windows
packaging fixture passes including the recomputed-manifest/missing-art negative
case. STATUS now names the actual published v0.3.4 and the unpublished v0.3.5
candidate. Candidate release notes and the open RX550 report are explicit.

## Actual original Add-On bundle refresh

The attempted full reimport cannot reproduce the pinned custom Tier1 original
from this machine's currently discovered alternative copies. No pin was changed
and no shipped Add-On was dropped. Reuse the independently verified exact
v0.3.4 native bundle, then replace only the Player Throwing original and its
companion with the ordinary current importer output from its pinned source.
The original component is byte-identical; the only changed payload across the
entire bundle is addons/script_playerthrowing-rules/throwing.rhai. Its recovered
numeric bindings remain identical to the old file. A template containing
{{parameters}} is deliberately not byte-equal to its generated rule; that
initial comparison was corrected, not treated as a loader defect.

The refreshed archive includes 100 originals and 160 packages/companions. Every
archived byte was compared with its assembled input. SHA256:
fdb699a92dbc99bc8321409d0eeee0ebddc65d89064447390008a8cb9610a571.
It is uploaded as addon-bundle.zip on the existing private draft release. The
separate UI-v5 base snapshot was already byte/semantic verified and uploaded to
the private ci-content draft. No original, generated, decompiled or reporter
payload is committed. Original installations stay read-only.

## Next supported work

The complete focused run passes. Commit the callback correction and current
notes, then build optimized matching
binaries, assemble a fresh local public candidate with the refreshed Add-On
bundle and UI v5, verify the actual ZIP/extracted executables, and run the
DX12/Vulkan/software world matrix. Measure Loading Map in a quiet optimized run
before making any speed claim. Then gate/Windows CI, merge to main and run the
ordinary platform release workflow with actual artifact verification before
publication. The dirty primary checkout is preserved; use the clean integration
checkout and workflow dispatch rather than checking out over user changes.

The existing private Interdiction preview already selects both Pipeline and
Interdiction modes (94 packages total). A fresh matching preview can preserve
those private assets/selection while updating the base/UI and public Add-On
inputs. Run the existing full-map Pipeline and Interdiction mission consumers
against the candidate, keep their fidelity limits explicit, and issue a fresh
versioned private package/receipt after main integration. Do not replace the
existing playable preview until the new package verifies.

GitHub asset metadata independently confirms the uploaded ci-content and
addon-bundle digests match the verified local archives. A fresh private input
snapshot retains both missions and all 94 selected packages: 12,679 existing
files were compared; only packages.json and the generated throwing rule differ,
with UI v5 added. The input control UI v4 is retained; the private packager now
copies selected roots into its fresh output rather than deleting input data.
Private packaging candidate 43c18395f supports manifest-verified public binaries,
separate previous state and preserved original credits; actual package exercise
remains pending the verified public artifact.
