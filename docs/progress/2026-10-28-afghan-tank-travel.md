# 2026-10-28 Afghan tank travel

Maxwell asked for a root-cause correction and a PR to remote main, with actual
tank tracks on his TDM Afghan build compared with useful travel through midfield
and toward the opposing base. Work starts from `origin/main` `72632dfec` on
`codex/afghan-tank-navigation`. The main checkout and interactive game were left
alone. The existing environmental-interaction/navigation owners are reused; no
content, wire or save schema changes, map-specific routes or AI control backdoors.

## Input and acceptance

The read-only local `TDM Afghan.world.json` has SHA-256
`853efb27dd74d1dc36ff8d3c8e6e9e066527cfe991e93a79a6b0a67edaedf61d`.
It contains 14,107 bricks, two stock tanks, saved Slayer CTF rules, eight slots
per team and rooftop flags. The source save and generated v20 content stay out
of Git. The headless trace restores the saved game for its recorded owner,
retaining loadouts, damage, teams, objectives, ordinary boarding, weapons and
vehicle destruction/replacement. It injects no bot tasks or controls.

Expected travel is through vehicle-sized streets to useful ground approaches,
then ordinary dismounting/walking for a feet-only rooftop objective. Parking at
spawn, confusing a partial search with arrival, and attempting to drive onto a
roof do not satisfy that expectation. Combat, an obstructing ally or a genuine
cliff can still justify stopping. Optimal paths and subjective tactical feel are
not established by a headless trace.

## Causes and correction

- A retained pedestrian route/search could survive boarding or leaving a
  control seat, including jump waypoints. Routes now belong to the body that
  planned them; a change clears the route/search/bridge, preserving its goal.
- Local driving safety swept a box around visual bounds. Empty corners beside
  the real tapered hull reported an adjacent building as penetration. Sweeps now
  use the instantiated, scaled and posed chassis/attachment collision shapes.
  Separating or tangent support contact does not forbid departure; real forward
  obstacles still stop the driver.
- The grid reserved a square as wide as the vehicle diagonal (about 8.1 units
  for this tank, versus its 4.87-unit cross-section), rejecting usable streets
  and parked poses. It now proposes cross-section corridors; actual hull sweeps
  retain local collision/turn authority. A bounded, retained forward/reverse
  departure can join the grid from a legal parked pose outside that abstraction.
- Fully extended wheel suspension was treated as the navigation floor, placing
  the start below the actual supporting spawn plate. The start now samples the
  supporting world surface while the cached navigation body remains stable.
- Every new seat claimant could renew crew waiting. Assembly now has one
  bounded deadline per ride, and distant seat offers stop after that deadline.
- A zero-progress conservative segment could fail the objective before vehicle
  recovery ran. Driver recovery retains that objective within its existing
  deadlines. Partial vehicle search segments now continue for pursuit, remembered
  targets and exploration as well as objectives; an intermediate search limit
  is not arrival. Unproductive recovery still relinquishes the vehicle.

## Evidence and reproduction

The original-code, two-minute headless baseline recorded maximum displacement
from each tank's spawn of **17.46** and **10.92** units. Spawn separation is about
167 units. Of 193 sampled driver decisions, 151 reported a hazard, 95 crew
waiting and 95 no next travel point; reasons overlap. This reproduces the report,
not a replay-equivalence claim for Maxwell's recorded session.

The local `artifacts/afghan-*.json` and logs preserve successive interventions,
including negative results: route invalidation or actual hulls alone did not
solve travel. A support-floor correction allowed the southern tank to travel
131.74 units in one three-minute candidate. Cross-section corridors allowed the
northern tank to travel 147.33 units. An earlier description inferred a round
reset from replacement vehicle IDs; that was not established by canonical round
results and is not claimed. An earlier 600-second candidate match completed,
including combat, destruction and respawns. Tracks group observed spawn locations
across replacement vehicle IDs (not treated as one immortal vehicle):

| Observed travel | South spawn | North spawn |
|---|---:|---:|
| Original code, first 120 s: maximum displacement | 17.46 | 10.92 |
| Corrected code, same first 120 s, including replacements | 58.23 | 147.33 |
| Corrected code, 600 s, including replacements | 76.17 | 147.33 |
| Corrected code, closest horizontal distance to opposing rooftop flag | 74.15 | 25.27 |

The north tank follows the east-side street corridor through midfield into the
opposing base area. South-spawn replacements advance along the west street into
the central combat approaches; the longer run reaches `z=-4.1` from `z=64`.
This candidate demonstrated that these physical street corridors are usable;
it did not establish robust departure across later recovery changes or promise
that both crews always charge the enemy flag. Live combat changes destinations.
The local `artifacts/afghan-travel-comparison.png` compares equal two-minute
windows; `afghan-after.json` retains all 600 seconds. Initial buildings in the
plot do not imply that damage or round resets were disabled. No diagnostic
prints remain in runtime code.

The committed `bot_vehicle_routes` probe writes positions, rotations, velocity,
wheel contacts/suspension, occupants, selected goals, next route points, partial
endpoints and objective diagnostics once per simulated second. With no supplied
local content/save it explicitly skips. It rejects missing saved game state,
empty vehicle reports, control-step failures and runs outside 1..=600 seconds.

```powershell
$env:BRI_CONTENT = '<existing generated content directory>'
$env:BRI_DRIVE_WORLD = '<absolute path to TDM Afghan.world.json>'
$env:BRI_DRIVE_REPORT = Join-Path (Get-Location) 'artifacts/afghan-travel.json'
$env:BRI_DRIVE_SECONDS = '600'
cargo test --profile playtest -p bri-chaos --test bot_vehicle_routes -- --ignored --nocapture
```

Source-free regressions cover body transitions, partial drive segments, tapered
hull corners versus real obstacles, supporting contacts, a raised spawn plate
and bounded crew assembly. Early fixture spawns inside the raised floor were
corrected without weakening assertions. Initial probe output used a relative
path under Cargo's crate working directory; the report now creates its parent
and the reproduction supplies an absolute path. A per-target LTO override failed
to link bitcode-only dependencies; subsequent diagnostics use the repository's
existing `playtest` profile. Temporary per-tick investigation prints are removed.

The broader pass subsequently exposed a retained failure after the blocked-driver
recovery correction: `afghan-gameplay-before.json` observes only **6.23** units
of south-tank displacement over 180 seconds, versus **123.55** for the north tank.
South crews repeatedly board, find no next point and relinquish the controls.
That failure kept the tank task open; the earlier long run could not be called
final acceptance. Drive-start diagnostics subsequently identified and corrected
the low-step support island described in the gameplay handoff. In the reviewed
ten-minute candidate both spawn cohorts cross midfield; actual replacement
vehicles reach 156.19 units from the south and 123.01 from the north spawn.
See [the gameplay pass](2026-10-28-bot-gameplay-pass.md).

Validation: the vehicle-interaction suite (24 cases including the added roof
regression), seven collision/contact tests, four route-lifecycle tests and the
quick/focused tiers pass. The full integrated gate remains pending for the
candidate that includes the requested sound work.

Maxwell's interactive acceptance and the complete alpha contract remain open.
