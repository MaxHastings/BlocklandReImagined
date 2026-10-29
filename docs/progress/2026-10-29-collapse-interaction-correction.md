# 2026-10-29 Recoverable large-collapse interaction

Maxwell requested a proper correction for oversized rigid chunks, floating
assemblies and interaction lost under resource pressure, followed by merge to
main when satisfied. The preceding [review](2026-10-29-collapse-gameplay-review.md)
records the inspected baseline. Work uses `codex/collapse-interaction`, reusing
the destruction worktree from current main `030b43a03`.

## Correction and acceptance

Connected landing groups are independent of free body slots: at most16 members
and six world units on each axis. These are initial gameplay tuning choices,
not source ABS fracture laws. An already oversized single authored brick stays
whole, without adding its neighbors. Landing separates the coherent section
into these groups; this does not add general secondary material fracture.

Keep the64 resident chunk-body and2048 resident-proxy-part limits and the
existing Physics Quality body budget. Overflow groups retain exact compound
proxies and collision-checked translation, including gravity, map/terrain/live
brick contacts, other rubble and player/vehicle contacts. Resident and overflow
rubble share a spatial contact snapshot. The cheap path holds orientation
instead of solving simultaneous angular constraints; it is not a frozen render
transform. Full solver admission resumes at the current pose/velocity.
Quality eviction and insufficient cached surroundings preserve motion state
and return to this path. No permanent shared-only representation remains.

The512-query-equivalent section collision allowance is divided between window
preparation and overflow contacts; cheap metadata visits cost1, real casts16.
Queries and group scheduling remain resumable. Exact face merging preserves
holes; retirement rebuilds only affected collision proxies. Lifetime, rendering
material, source identities, authority, wire/save formats and content are unchanged.
Extreme demand can still reduce update frequency; this is not an unlimited
skyscraper/FPS claim. Existing non-box bounding-proxy approximation remains.

Actor admission uses body extents rather than only centers. Overflow contacts
also sweep the actor's previous/current path, covering crossings between frames.
Projectile hits change velocity through checked movement instead of shifting
chunk artwork by an unchecked recoil offset. Preparing shared sections retain
their existing local presentation response.

The integrated100-disconnected-member scenario exposed an additional actual
flight-clock stall: `age=0.4166668`, `confirmed=0.43333334`, known impact
`0.42001894`. Requiring a whole confirmed frame made `age+dt` round past the
confirmed horizon, so it never reached the already proven contact. The clock
now consumes available confirmed partial time and crosses that contact.

Acceptance covers endpoint actor admission; connected1500-member integrity and
bounded groups; >64 disconnected groups; one-slot planning and later recovery;
zero-slot gravity/player response and new-wall collision; airborne quality
eviction; support removal; compound openings; mixed resident/overflow rubble
stacking; swept actor crossings; projectile response; source retirement,
late delivery, restoration and reset through existing checks.

## Verification ledger (in progress)

- Initial section run:20 pass/two failures. The support fixture was off its
  authored plate grid (`2.0` rather than `2.1` for a three-plate member); it
  now uses valid coordinates, retaining its contact/drop assertions. The older
  overload fixture spanned298 units beyond its100-unit-wide floor and did not
  guarantee landing; it now uses a10x10 disconnected grid above the floor and
  requires independent landed material, replacing its obsolete ghost assertion.
- The next run caught the real confirmed-clock stall described above. Two
  projectile test failures followed removal of immediate render recoil: the
  bodyless test now observes checked movement over12 frames. An accidentally
  inserted delay in the sleeping-body immediate-impulse test was removed,
  retaining its original wake/speed assertion.
- `cargo test -p bri-client --lib brick_debris::sections:: --locked`:
  24 pass in0.23s (`artifacts/collapse-sections-sixth.log`). Includes zero
  slots, real openings, mixed-pool rubble contacts and the clock regression.
- First `tools/check.py focused --crates bri-client` stopped on three new
  Clippy diagnostics (argument count, nested if and map-entry use). Corrected
  with a motion-frame context and a single map entry; no lint suppression.
- `python tools/check.py focused --crates bri-client --tests bri-client:brick_respawn`:
  formatting, warning-free all-target Clippy,757 client units (78 existing
  ignores), synthetic native QUIC host/joiner brick restoration pass; generated
  content integration variant remains explicitly ignored.153s total; log
  `artifacts/collapse-focused-second.log`. This predates the final swept-actor
  extension and is not final-head evidence.
- Seven new interaction controls were compiled and run against original
  `030b43a03`: all seven fail their behavioral assertions, not compilation
  (`artifacts/collapse-failing-baseline.log`). The bounded baseline script
  restored every implementation source byte in `finally`.
- The swept-actor extension exposed loss of tangential travel at a floor hit.
  Overflow motion now performs up to three checked slide continuations through
  the existing quota; a floor does not consume a sideways shove, and the next
  wall/corner still receives a shape sweep. Later blasts also impart checked
  velocity to overflow rubble without new bodies. The resulting26-section
  selection passes in0.24s (`artifacts/collapse-sections-slide.log`).
- Final implementation: `python tools/check.py focused --crates bri-client
  --tests bri-client:brick_respawn` passes in135s: formatting, all-target
  warning-free Clippy,759 client units (78 existing ignores), and the native
  QUIC host/joiner synthetic restoration integration (one generated-content
  variant explicitly ignored). Log: `artifacts/collapse-focused-verified.log`.
- `cargo run -p bri-client --bin bri-client --locked -- --check` exits0 after
  the native Windows executable build. The local converted content reports227
  fallbacks and optional missing derived lighting for community maps; those
  warnings concern unchanged imports/lighting, not this debris correction.
  Log: `artifacts/collapse-content-check.log`. No source installations changed.
- `python tools/check_docs.py` passes76 maintained documents and six generated
  guide layouts; `git diff --check` passes. No whole-workspace/extended gate,
  performance campaign or interactive acceptance is claimed.

No interactive game was launched or automated. On release-root notice of an
active v0.4.1 playtest, existing tests had completed and new heavy work paused;
work resumed only after Maxwell told the release chat playtesting had finished
or paused. This lane does not publish a release. Root coordinates the
v0.4.2 lanes and final combined release checks. Technical completion, main merge
and Maxwell's later gameplay acceptance must remain distinct.

Implementation and scoped technical acceptance are satisfied. Next: commit,
history/protocol checks, exact-head platform CI and coordinated guarded main
merge. Maxwell still owns perceptual gameplay acceptance; the existing broader
alpha contract and root's release gate remain open.

## Final actor-selection review

Review of the new selection code found repeated all-rubble distance scans in
the sort comparator. It now measures each actor once and sorts cached values.
Resident and overflow paths share the same32 selected actors; the resident
path previously reran its old ID-first truncation and could exclude a touching
actor in favor of farther low IDs. The existing endpoint regression now adds
a32-actor farther crowd and requires the nearby high-ID actor to keep contact.
No new body, pusher or query limits are added. Original head `1c1ebda82` passed
the committed-tree scoped gate (tool tests7s, docs1s, focused154s); that receipt
and its CI are historical, not validation of this follow-up.

The coordinator owns release-wide lane selection and final packaging.
This lane still owns only client debris/contact code. A read-only audit of the
authoritative restoration path was handed to root; bots own the independently
proven occupied-space restore correction. No authority/audio edits were made
here. Main mutations remain serialized by root.

The selection follow-up passed focused checks (759 units,78 existing ignores,
warning-free all-target Clippy and synthetic host/joiner restoration,166s;
`artifacts/collapse-selection-focused.log`). A bounded control against1c1's
actor path fails the new crowd-contact assertion, then restores the updated
implementation byte-for-byte (`artifacts/collapse-selection-failing-before.log`).

Extending the existing removed-fragment check through a delayed Ready barrier
also exposed an actual collider lifetime bug: an already inactive member could
be included in a proxy constructed later. The test fails on actual shape point
containment (`artifacts/collapse-staged-retirement-failing-before.log`), not a
render flag. Finalizing a group now filters retired members after charging its
original planned work; no invisible material or free unbounded scanning is
introduced. This is still client presentation retirement, independent of the
bot lane's authoritative occupied-space restoration correction.

Final follow-up verification: `python tools/check.py focused --crates bri-client
--tests bri-client:brick_respawn` passes formatting, warning-free all-target
Clippy,759 client units (78 existing ignores) and synthetic QUIC host/joiner
restoration in300s. The generated-content integration variant is explicitly
ignored as before. Log: `artifacts/collapse-followup-focused-final.log`.
The earlier native startup/content result is reused: these two follow-ups do
not touch startup, content or packaging. Fresh-head platform CI still must
build the shipping executables before merge. Source is frozen after these two
supported final-review corrections; no further audit campaign is part of this task.

Finite delivery is merged PR405 plus the verified technical handoff to the
release coordinator, who owns the combined playable package. After the package
contains this correction, Maxwell's concrete feel checks are: make an ordinary
wall/floor hole; sever a large mast/roof after leaving older debris active and
walk into its pieces/ends; repeat at low Physics Quality, hit the rubble again,
and observe restore/reset. Expect bounded breakup, continued gravity/contact,
and no restored invisible proxy. These human checks are not marked accepted
by the native/unit evidence above. General fracture, arbitrary-size/FPS promises
and unrelated physics changes remain outside this finite correction.
