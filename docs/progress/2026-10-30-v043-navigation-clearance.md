# Navigation body clearance investigation

Previous goal turn made concrete progress: committed frontier and pursuit
continuity corrections, with ordinary-control kills in three supplied worlds;
completed the official post-v20 announcement register; delivered editable current
ambience loops at Maxwell's request. This turn continues the two failing worlds.
Audio replacements are pending Maxwell's edits; independent implementation proceeds.

The farther house-egress control still wedges a zombie at approximately
X=96.374, repeatedly jumping/crouching against the window, then respawning.
Added a bounded `--every-tick` option to the nonshipping saved-world probe
(maximum 3,600 ticks). Actual input/body transitions confirm zero forward progress
through repeated jumps; this is not merely a lost-target timer.
Private trace: `diagnostics/v043-house-egress-tick-trace.json` under the planning
evidence root. A static audit correctly refused this particular setup because
its only bot is already airborne; it did not invent a grounded pose to obtain
a passing route. An initial command had an incorrectly quoted apostrophe in
the save filename; corrected without touching the input.

Confirmed a generic clearance mismatch independently: pedestrian graph clearance
shrinks body width by one grid cell. The full motor body is 1.25 units wide, yet
the graph admits an opening narrower than that body. The new regression first
fails with `grid alignment cannot make the authoritative body narrower`.
Full-width clearance makes that regression pass and exposes two retained graph
tests (overlapping open stair treads and descending a ledge shaft). The first
full navigation run is **42 passed, 2 failed, 1 existing ignored measurement**;
do not report this increment green or weaken those assertions.

The ongoing correction keeps actual motor width and resolves grid placement
through existing bounded within-cell settling. Stair support may be under the
front of the body above the centre ray's tread: a full-body down cast within
the motor's existing measured step can discover that real support instead of
shrinking the body. This is being tested against the retained stair/ledge cases
and must be followed by actual saved-world traversal and work-budget checks.
No clearance or cube-stacking acceptance is closed yet.

The first support-cast correction restores overlapping stair-tread coverage;
the ledge descent still fails (43 passed, 1 failed, 1 ignored). Its last route
starts a jump before the full body clears an upper overhang. A body can stand
with its centre just beyond a ledge while its footprint remains supported;
centre-only floor rays omit that legal departure. The next correction samples
full-footprint support within the existing step/drop bounds, retaining real
body clearance and the existing per-tick node budget. Validate this against
the retained ledge fixture, not by changing its expected outcome.

Open-issue refresh also found #416 (borderless/window resolutions) and #417
(optional automatic draw distance). Maxwell's original instruction covers open
GitHub issues, while the approved execution plan requires explicit triage of new
reports. Inspect the existing display/quality owners for bounded corrections and
record the scope decision before adding acceptance. Do not copy Torque-specific
settings blindly or change manual draw distance without an opt-in setting.

Current ambience edit copies are byte-verified, unconverted preview OGGs:
indoor `7c8cd5cd5ddd41003b59a23645564313213f2880a657932e0765eb5acc293845`,
outdoor `360e42c4472cf74e15a8bc0a12f12083a342282fe8bd96f6a5b3a0cce488a0a0`.
They already contain the requested roughly +4 dB change over v0.4.2. Preserve
the originals and integrate later edits once, without applying that gain again.


October 31 continuation: the full motor proves both disputed edge transitions.
A new device-free regression starts a full-size player with its centre half a
unit beyond a platform edge, settles it on its actual footprint, then executes
the ordinary leap controller without jets. A second actual-motor case leaps
from the 4.8-unit ledge to footprint support on the 2.4-unit ledge under the
7.2-unit overhang. Both land at the requested height and within one cell of
the target. Thus rejecting all footprint-only landings was too conservative;
adding a new falling controller is unnecessary for this retained case.

The graph now considers those supports while continuing its scan for a centred
landing. Existing SNUG cost prefers stable centred support; this planning cost
is separate from the measured flight time and execution timeout. The old gap
fixture required the departure centre to remain inside its platform even though
the real motor supports departure half a unit beyond it. Its assertion now
requires the full footprint to remain supported; the target-centre, gap count,
height and too-wide negative checks remain. This is a physically demonstrated
contract correction, not an unexplained tolerance increase. The nav suite at
that step passes 45 tests, with its one pre-existing ignored measurement. A
subsequent cost/time representation cleanup still needs the final same-suite
check, Clippy and actual saved-world reruns before this increment is complete.

Maxwell subsequently confirmed he will keep the current ambience loops; no
replacement/edit is pending and no additional audio gain should be applied.


Final same-suite check after separating cost from measured time: 45 passed,
0 failed, 1 existing ignored measurement. Changed sim/net all-target Clippy
passes with warnings denied. `cargo test --locked --profile playtest -p bri-sim
--test nav_invalidation` passes its actual per-brick invalidation case. The
rebuilt native probe loads all bricks with zero set-aside and zero bot-step
failures in the four rerun saves. It records target deaths at ticks 3240
(staircase), 4973 (city tower), and 4930 (mattress house); those are actual damage
outcomes, not static path claims. Reports are private
`diagnostics/v043-{stairs,tower,mattress,house-egress}-clearance.json`.

The farther house-egress control remains alive after 3600 ticks and is not
accepted. Unlike the earlier repeated jumping, its latest trace abandons chase
while retaining visible target knowledge. Existing `out_of_reach` policy can
suppress a valid partial route before the bot executes it. That is a separate
pursuit-policy correction; this clearance commit does not claim to fix the
window, stacking, all natural stair centering, or the complete bot workload.
