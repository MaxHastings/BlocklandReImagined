# 2026-10-24 Interdiction invasion final branch handoff

Maxwell explicitly requested stopping new work, pushing the current candidate,
cleanup and a copy-paste handoff. This is a branch checkpoint, not completed
mission acceptance or a main merge. The full dependency/evidence index remains
`2026-10-24-invasion-wrap-handoff.md` and the private `aa-interdiction/HANDOFF.md`.
Final pushed commit identities accompany the external handoff receipt.

## Latest completed evidence

Private evidence is under
`../BlocklandReImagined-private-evidence/interdiction-ui-lighting-20261024/server-lag-report/`
relative to the main checkout's parent.

- Quick 155 passes formatting and library tests of content/motor/sim/net.
- Cache installer smoke 156 passes the actual runtime decoder/query adapter,
  with `--no-install`. Cache metadata now explicitly disclaims unproven upstream
  provenance instead of crediting the current baker to arbitrary cached bytes.
- Fresh bake 157 compiles the owned C++ baker against external pinned Recast
  `9f4ce64458dfae86e1239c525ddc219c4e9e06f1`, exports native fixed collision,
  and reproduces the retained 25,849-polygon mesh **byte for byte**:
  SHA256 `fb599cb6dfa35af647fca47034c87585e5be381ae61ccc3f3b0d29fc24bfa4df`.
  Runtime queries retain the established rejection of NPCPathNode108's initial
  wrong-floor attachment; this is not weakened to match upstream's static query.
- The final compiled gameplay runtime passes forest descent 158: all four
  naturally spawned zombies arrive at the deep source target in 193, 199.5,
  202.5 and 225.5 simulated seconds. No supplied route/teleport/suicide or relaxed
  arrival threshold is used. Other actors are explicitly removed by the
  traversal fixture; this remains separate from battle acceptance.
- Patrol pair 153 passes both soldiers at 3.5/4 seconds, versus one still stuck
  after 120 seconds in negative 150. Sparse/dense rejoin 152 fails before and
  passes after. Door first-step rollback 144 fails before and 146 passes after.
- Mixed battle 154 completes 180 simulated seconds with the same compiled
  runtime. Mover110/Mover153 open by 2/1 seconds. The unchanged reviewer's
  longest flagged commanded-but-stationary interval drops from 75.5 seconds
  (141) to 8.5. Session median/p95/p99 are 3.922/23.055/33.614 ms; maximum
  168.965 ms. This excludes QUIC, snapshots and trace writes and is not complete
  battle-flow or latency acceptance. Retain the remaining anomalies.

Focused 159 finds only three style errors (collapsible condition and test-module
placement). They are corrected without changing route decisions. Focused 160
is the retained rerun log: formatting and clippy pass. Its expanded 28-crate
dependent-test compilation was intentionally stopped at Maxwell's explicit final
wrap request. The focused tier is **incomplete**, not passed; see the separate
`navigation-focused-160-STOP.json` receipt. No full gate or new two-client latency
pass is claimed. This is a pushed branch checkpoint, not a merge-ready claim.

## Private producer changes at the stop boundary

The portable installer now checks required arguments before building, uses
atomic JSON replacement, records actual input hashes, and refuses an interior
scene until explicit floor-lift coordinate handling exists. Current AA scene
contains only static models/lights/spawn/climb/sun, so the tested path has no
interior floor lift. Both successful installer runs leave content unchanged.

The old external immediate-preview hook is preserved as private authored
`policy/invasion_preview.rhai`, exposed by `install_invasion.py --immediate`.
Without that option the source mission TimedQRF trigger remains. Script output
uses exact UTF-8 bytes matching its receipt hash. Syntax compilation passes;
this newly exposed option has **not** been recomposed and exercised as a new
assembled package. Do not describe the option as delivered or accepted.

## Resume from here

1. Use the two preserved worktrees/branches and final remote pins; do not create
   a fresh worktree or restart the strategy assessment.
2. Recompose a fresh candidate from a verified ordinary co-op base. Compare
   source spawn/admission functions with content 3 before accepting the result;
   do not assume an older assembled base contains the latest ambush correction.
   Install the reproduced navigation and receipt the current producers.
3. Run the existing actual two-client QUIC path against that candidate and its
   explicit immediate preview mode. The existing latency test defaults to
   ordinary co-op and cannot certify zombie load without adapting its scenario.
4. Review remaining 154 battle anomalies as physical/route/action evidence.
   Do not add a crowd framework or relax acceptance to hide a failure.
5. Finish review, required checks and the full gate before main integration.
   Use `python tools/gate.py --push`; never bypass the hook. PR merges require
   the repository's Windows CI. Public v0.3.6 stays immutable.
6. Warm-build the client, verify the assembled package and replace only the
   obsolete immediate preview. Preserve user identity/settings/admin. Maxwell
   performs all interactive gameplay. No replacement build exists at this stop.

Keep both worktrees and their warm targets: they are unmerged and necessary for
continuation. Temporary private net-test modules were restored/removed. Preserve
all external evidence, scientist stashes and other threads' work. Do not retry
the earlier denied optional content-directory deletion. No broad cleanup is
required to save the branches.
