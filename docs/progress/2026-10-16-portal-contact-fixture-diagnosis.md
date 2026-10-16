# 2026-10-16 Portal vehicle contact diagnosis

Root traced the remaining allowed actual native portal test failure on committed
candidate3436de89 without modifying production or candidate HEAD. Temporary test
clone used actual Session/native and synthetic fixtures, logging car poses and
source walker motion every20ticks. Log artifacts/root-portal-contact-trajectory.log.

The native car DOES meet the projected player: near tick160 its velocity changes
and the source walker moves from z=-24.002 to -22.680, then -20.855 as the hit
push is mapped back through the portal. The car later enters the opening after
pushing the finite-mass walker away. Existing share_through_openings deliberately
shares contact impulses with the source player. The synthetic lighter car nearly
stops and gives the walker a much smaller push.

The old fixture assumes every car permanently stops against that player, contrary
to the existing finite-mass collision policy. It also measures x-coordinate nose
after teleport when x is already in the other portal's coordinate frame; this
explains the reported -3.057, not disappearance of contact. This is an evidence
based fixture diagnosis, not a production collision correction or release pass.

Root tested a replacement observation in the temporary clone: compare occupied
versus empty opening trajectories only while both cars are still in the approach
coordinate frame, require meaningful collision-induced delay and a correctly
mapped positive source-world player push. Preserve the empty-opening advance
check; measure nose before teleport. Positive run2/0/0,66filtered,0.19s:

- synthetic maximum approach lag2.660408, source push.10254288;
- native lag.92962265, source push3.146574.

Negative countercheck places the alleged occupied-case walker laterally outside
the opening and omits only its in-opening precondition. Both actual native and
synthetic replacement assertions fail for exact no-response lag0:0passes/2fails,
exit101. Logs root-portal-contact-{response,negative}.log preserve both. Standard
no-host-grip warnings in these isolated fixtures remain, not hidden.

Prepared artifact portal-contact-response-fixture.patch targets the tracked test
only, without traces/negative mode. It is NOT applied to tracked code or current
gate; apply in next atom with scoped formatting/focused+native checks and remove
obsolete allowance only after that evidence. Temporary owned test clone was
removed after all three completed command sessions82201/51929/88944. Main and
candidate remain unchanged; no broad cache cleanup or other checkout alteration.

Public PR173 candidate gate still runs455binaries after successful build/content/
Clippy. Current allowance is still part of that frozen candidate; root will not
rewrite its identity mid-validation. All original fidelity/release hold remains.

## Applied and validated closure

On root branch codex/v031-camera-portal-integration from publicmain3436de89,
root applied the same inspected replacement to the tracked fixture. The saved
unified patch failed to apply directly, so apply_patch applied the exact hunks;
no production path or timing budget changed. Quick403/0/4 in99s. The actual
tracked test, native and synthetic, passes with --include-ignored:2/0/0 in.17s.
Only after that native success, the obsolete known-failure allowance was removed.
Focused8steps passed in1659s:197 successful test summaries totaling2346passed,
0failed,251ignored; sim all-target Clippy -D warnings passed. The separate
native run supplies content evidence the focused tier normally ignores. Logs:
artifacts/v031-portal-fixture-{quick,native,focused}.log. No acceptance threshold
or total test count was reduced; previous no-contact negative controls still
provide the sensitivity evidence. Full integrated candidate gate and Windows
checks remain required before main integration.
