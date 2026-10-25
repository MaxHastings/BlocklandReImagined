# 2026-10-24 Interdiction invasion wrap and continuation handoff

Maxwell requested a wrap and substantial handoff, not another architecture
investigation. This entry is the continuation index. The private experience's
`aa-interdiction/HANDOFF.md` retains source-specific acceptance and older evidence.
Do not restart that research or discard the corrections already made.

## Delivery and ownership

- Engine worktree: `../BlocklandReImagined-worktrees/zombie-hole-20261023`,
  branch `codex/interdiction-server-latency-20261024`, HEAD
  `c2fa7dc4bf998c882d3a59775257bf75c3cefcf2` plus working changes.
- Private worktree: `../BlocklandReImagined-worktrees/interdiction-doorway-private`,
  branch `codex/interdiction-zombie-invasion-20261024`, HEAD
  `6252c9b8825a24eba80a7af5ad48744d53916501` plus working changes.
- Remote main verified by `git ls-remote origin refs/heads/main` on October 24,
  21:17 America/Chicago: engine `0752b9f3dad711ad38c4316921ebda959ac2bf73`,
  private `e022acd7ae34e7dde3536e414f3fec155ac87e2a`. These match the previously
  fetched bases. Recheck before integration; working-tree changes are not merged.
- External evidence root, relative to the main checkout's parent:
  `BlocklandReImagined-private-evidence/interdiction-ui-lighting-20261024/`.
  Runs below are under its `server-lag-report/` directory.
- Final working patches, untracked source bytes and SHA256 inventory are archived
  in `server-lag-report/wrap-checkpoint-155-inputs/PINS.json`; source matching
  against compilation 153 is checked before archiving. A standalone copy of this
  handoff is `server-lag-report/WRAP-HANDOFF-20261024.md`.
- Undelivered assembled candidate: `invasion-content-3` in that evidence root.
  Keep `invasion-content-2` as the delivered negative control.
- Existing public v0.3.6 remains immutable. No replacement playtest or new merge
  is claimed by this handoff. Both worktrees are unfinished; preserve their warm
  targets and all unrelated work. Do not run broad cleanup.

The obsolete delivered preview is main checkout
`dist/AA-Interdiction-Zombies-20261024-Immediate`, desktop shortcut
`Interdiction Zombie Playtest.lnk`. Maxwell permits replacing it when ready.
Its engine is `02f7e86049c4a56af68e1a1ccea8464f5c5b5e4e`, executable SHA256
`6abeab19b6fd66e0e0d625a84031c47aacefcf643379f96c14c0005402d8a2af`.
It does not contain this working candidate. Preserve identity/settings/admin
when replacing it. The old `deliver_zombie_immediate.py` hardcodes old pins and
content 2; do not rerun it unchanged.

## Gameplay gains supported by evidence

| Outcome | Evidence and limit |
| --- | --- |
| Zombies begin in the forest instead of inside the bunker | Old placement fails `forest-spawn-before-34`; four starts pass ground, overhead-clearance and outdoor-region checks in `forest-spawn-after-48`. |
| Defenders use original grouped ambush posts rather than a pile of house markers | Same assembled comparison fails `ambush-spawn-before-30`, passes five starts in `after-35`. Exact source collision-retry order and random walking admission remain incomplete. |
| Defenders reload after emptying their weapon | Before 21 remains empty despite spare ammunition; after 23 moves 300 spare to 270 spare plus 30 loaded. Human manual reload is retained. |
| A shallow mesh contact no longer prevents moving away | Two-sided mesh negative control fails 103; separating motion passes 104/105, inward motion remains blocked. Cuboid-only controls had missed this. |
| A moving pursuit anchor no longer sends the follower down the obsolete route | Moving-anchor comparison fails 53, passes 54; final floor arrival remains strict. |
| Four naturally spawned forest zombies can independently descend to the deepest facility target | Runs 139 and 140 pass all four through ordinary controls, with no externally supplied guides, relocation, suicide or softened arrival criterion. Run 140 arrival times: 190.5, 202.5, 205.5 and 229.5 simulated seconds. |
| A bot can clear an obstructed door swing and resume opening | Resume regression fails 132, passes 133; contact/retreat/open/cross comparison passes 134. Run 135 proves Mover90 fully opens, but still fails one zombie at its open leaf. Local repair in 139/140 closes that traversal failure. |
| An opening blocked on its first actual movement keeps its target | Bot-phase-order negative control 144 loses target 1 to 0. Corrected rollback passes all four mover/action checks in 146. Mixed-battle confirmation remains separate. |

These are bounded technical/integrated observations, not Maxwell's acceptance
of complete smooth battles. The mixed battle still contains consequential stalls.

## Current design: keep this simple

One prepared polygon corridor supplies global walking topology. The ordinary
motor executes it and remains authoritative for collision, stepping and floor
arrival. A bounded native local search repairs an observed intersection with an
operable door's current open leaf and rejoins the retained corridor. It does not
reconstruct every global leg, raise every bot's budget or replace locomotion.
Covered positional patrol/search goals and zombie pursuit share that provider.
Incompatible bodies and uncovered attachment locations retain the mature local
planner. Actual nearest-floor bounds are enforced; wrong-floor attachment is
explicitly rejected.

Engine mechanisms: portable validated `WalkingMap`, Detour adapter, body/profile
admission, route execution and repair, contact separation, mover rollback/resume,
targeted bot passage action. Add-On policy: source posts, finite defenders,
forest starts, invasion hostility/waves, opening requests and spawn lifecycle.
WaW library art, gait, four variants, 43 animations and 141 sounds are unchanged.
The invasion owns its AA-map movement archetypes; the asset library owns no AI.

`bots_open_target="1"` is an opt-in extension to existing `bots_open`. It
supplies the door ID to a declared string command. Commands without it retain
their existing path. The bot retains the passage while ordinary retreat clears
contact, waits for opening, then resumes; aim/firing are not intentionally paused.
Human UseControl toggle remains distinct. `npc_doors.py` composes the shared
policy into both ordinary co-op and invasion and has an idempotency check.

No crowd framework was added. The apparent Mover153 crowd failure also occurred
with one bot left alone; adding reservations/RVO would have hidden that evidence.

Final quick check 155 passes: `python tools/check.py quick --crates
bri-content,bri-motor,bri-sim,bri-net` runs formatting and library unit checks
for all four, eight steps, 244 seconds. The check log retains intentionally
ignored cases; it is not the full gate. Focused checks/review, fresh bake,
forest rerun on the final runtime and main integration are still pending.
The working patches are preserved without creating an unreviewed integration
commit. No check or capture is intentionally left running at this checkpoint.

## Last bounded correction

Natural patrol pair runs 148/150 keep only Gd17/Gd18, their original brains and
goals. One crosses; owner 16 remains at Mover153 after the partner leaves.
The repair trace requests a goal roughly 16 native units away, while the local
search is bounded to 10.56. Sparse corridor vertices allowed the old selection
rule to jump beyond the intended 7.04-unit rejoin horizon. Partial progress was
then discarded and the blocked coarse segment reused.

`navigation::repair_join` now samples along the existing corridor within that
horizontal path-length horizon, interpolates height, and retains its tail and
destination. It does not change budgets, acceptance or gameplay goals.
`sparse-rejoin-before-152.log` fails on the old rule; `sparse-rejoin-after-153.log`
passes both density-independent and bent/sloped/short-route checks.
The unchanged assembled pair comparison `bounded-rejoin-pair-153` passes:
owner 16 crosses at 3.5 s and owner 17 at 4 s after observation begins. Before
150, owner 16 remains stuck through the full 120 s and owner 17 crosses at
82.5 s. Original patrol brains/goals and the physical passage remain unchanged.
Exact modified bytes and hashes are in its `-inputs/PINS.json`. This closes
the isolated regression, not crowded mixed-battle acceptance.

## Performance: real improvement, incomplete acceptance

All three captures use a compiled private Session consumer, twelve initial zombies,
restored defenders and passive healed human beacons for 180 simulated seconds,
with no concurrent Cargo build. Decision changes cause different populations.

| Session step measurement | Before 136 | Prepared repair 141 | Final bounded fixes 154 |
| --- | ---: | ---: | ---: |
| Mean ms | 8.350 | 6.836 | 6.472 |
| Median ms | 5.926 | 4.243 | 3.922 |
| P95 ms | 25.090 | 23.626 | 23.055 |
| P99 ms | 37.305 | 35.063 | 33.614 |
| Maximum ms | 178.743 | 163.604 | 168.965 |
| Bot thread aggregate CPU ms | 105000 | 68531 | 67281 |
| Script call wall ms | 39391 | 40153 | 39068 |

These exclude snapshots, QUIC and trace writes. Windows thread CPU is quantized;
do not report its per-tick percentiles. Script counters measure call wall time,
despite the old `script_cpu_ms` label in capture 50. No universal speedup or
network-latency acceptance follows. Review full 3D engagement/stationary traces,
not only kill counts. Run 141's main outstanding sites are Mover110 and Mover153.
Final 154 completes 180 simulated seconds (last actor sample 179.5 s), with
Mover110 fully open by 2 s and Mover153 by 1 s. The same reviewer's longest
flagged movement-without-displacement interval changes from 75.5 s in 141 to
8.5 s in 154. This is a useful detected-flow improvement, not proof of every
passage or engagement. Remaining flagged cases include Gd52 outdoors at
[-222.882,-24.520,70.325] and Zw7 near blocked Mover87 at
[-116.738,-21.331,6.320]. Preserve their full underlying trace before assigning
a cause. Final populations are 39 defenders/10 zombies, versus 36/12 in 141.

## Reproduction and exact pins

From external `server-lag-report`, with the two worktrees above intact:

```powershell
python run_invasion_case.py <fresh-name> invasion_native_patrol_pair_passes_door153
python run_invasion_case.py <fresh-name> invasion_forest_group_reaches_deep_facility
python run_invasion_case.py <fresh-name> invasion_dozen_battle_flow_diagnostic --seconds 180
```

Names must be fresh: receipts/evidence are never overwritten. The runner saves
both HEADs, working patches, untracked source bytes/hashes, generated map/scene,
navigation, archetypes, scripts and mode inputs. It uses the canonical private
`aa25/tools/run_solo_rounds.py` fixture and temporary `--lib-consumer` module,
restoring its exact owned changes on completion. It verifies an actual test ran.
Do not edit runtime sources during compilation/capture. Running modules and
fixture-only commands must not ship in the preview.

For repeated timing, reuse the exact compiled test binary and record its digest
as in `run_compiled_battle_141.py`; update its linked compilation receipt and
fresh output names. Do not run Cargo alongside timing. The initial consumer
compile was 4m45s; warm incremental 153 is 1m09s, not instantaneous.

Offline converter sources are preserved in private `tools/install_navigation.py`,
`tools/navigation_bake/`, `tools/navigation_source.py`, with authored
`policy/walking-profile.json`. The rewritten portable installer has only Python
syntax validation so far. It still needs a cache smoke check and fresh bake
reproduction, including early argument checks/provenance review. Do not claim it
is ready or replace the proven generated candidate blindly. Exact working mesh:
`recast-topology-88.json.mesh.json`, SHA256
`fb599cb6dfa35af647fca47034c87585e5be381ae61ccc3f3b0d29fc24bfa4df`;
report `recast-topology-88.json`, SHA256
`cd24d2ea7f01716ccddde728aae4f73459b8a03327a6eb673bb52142bfb49ad5`.
External Recast checkout is pinned at
`9f4ce64458dfae86e1239c525ddc219c4e9e06f1`. Runtime wrapper dependency is
`recastnavigation-rs =0.1.0`; licenses/notices and package-guide copying are in
the engine changes. Original content/research clones remain outside public Git.

## Remaining work, in execution order

1. Preserve the passing 153 pair and failing 150 as a regression. Use the
   final 154 mixed battle's retained anomalies for the next correction; do not declare
   crowd avoidance the cause without separating route/physics/action failures.
2. Rerun forest descent using the corrected runtime. Final 154 already confirms
   both problem movers open; complete passage/engagement review remains separate.
   Review delays to firing, corners/stairs, door obstruction and actor bunching.
3. Finish only the needed offline bake/installation reproducibility checks.
   Recompose content with current producers and record a new truthful receipt;
   the existing content 3 producer receipt legitimately points to older sources.
4. Check actual network responsiveness through the existing two-client path;
   Session timing alone does not close the five-second delay report.
5. Run required focused checks/review, then the integrated candidate's full
   gate. Integrate using repository procedures; no unverified main push.
6. Warm-build `cargo build --profile playtest -p bri-client --bin bri-client
   --locked`, with CARGO_BUILD_JOBS=8 and CARGO_TARGET_DIR unset. Update the
   delivery script to current pins/content; verify the assembled package and
   overwrite only the obsolete preview. Maxwell performs the interactive check.

Animation catalog admission/frozen zombie, complete two-client animation
agreement, remaining visual/HUD/model/weapon fidelity and complete mission
acceptance remain explicit. Do not confuse those prior requests with completed
work. No subjective spectator-flow acceptance or fully faithful mission is
claimed. Avoid another broad strategy review; the next action is a concrete
retained failure or delivery check.

Preserve scientist stashes `7d32db71321607156277158bf0e6fa9b6e4d4a2d` and
`79f232eb08b555eb56cd1bec57a02fd93cfb3f0b`, with `owned-stashes.json` receipt.
Do not retry the previously denied optional deletion of `invasion-content`.
There is no need to delete evidence or a build directory to continue this work.

The older `2026-10-24-coarse-walking-guidance.md` is historical: its reverse
graph-field runtime was replaced by the prepared polygon corridor above.
`2026-10-24-navigation-motor-contract.md` retains the intermediate negative
experiments; read this current index before following any older pending result.

---

# Final checkpoint addendum


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
