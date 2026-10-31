# v0.4.3 movement capture and saved bot baselines

## Local movement evidence

The first preview adds a bounded `Movement health` line to the existing session
log every ten seconds and on a new body. It records authoritative tick/pose age,
input sequence/acknowledgement, current and peak pending history, peak sequence
trail and reconciliation distance, presentation correction, local/host feet and
velocity, grounded/jetting/mounted state and current movement buttons. Formatting
and I/O happen only at reporting time; ordinary frames update scalar maxima.
It changes no controls or reconciliation policy and sends no new wire fields.

This is evidence for the still-open local-feel report, not a declaration that
the separate guest input-admission correction solved it. Compare the log around
symptom onset and Maxwell's normal F8/F7 or respawn recovery. Preserve the
preview source/content identity and the session recording together.

`cargo test --locked --profile playtest -p bri-client --lib motion::` passed
28 cases; seven existing native-content variants remain explicitly ignored.
The filter also selects the existing crew-link movement case and related motion
names. Changed client/net libraries and `bot_scenario_probe` pass warning-free
Clippy. Scoped formatting passes. Initial compilation caught a mistaken diagnostic
field name (`strafe` instead of `right`); corrected before the passing checks.

## Native saved encounters

Added `bot_scenario_probe`, a nonshipping CLI using the actual dedicated host,
Load Bricks, saved-minigame restore and authoritative admin placement. It advances
ordinary controls and reports actual state/brain readouts each simulated second;
successful command completion does not mean a bot scenario passed. No window,
mouse, keyboard automation, new AI objective or private media is involved.

Inputs are the five SHA-pinned private saves in the approved plan, cloned v0.4.2
content with the documented v043 audio derivatives, and the normal enabled package
set. Bot spawn rotation is corrected; navigation is unchanged. Reports include
the environment digest, actual target pose, brick counts, unsupported map behavior
and every sampled bot's goal, route, input and explanation. The initial saved-game
spawn settles for 120 ticks before placement; otherwise it can undo the diagnostic
target position. The final cube run confirms its target remains at the requested
coordinates throughout the observation.

Twenty-second observations, after setup:

| Encounter | Loaded / set aside | Target outcome | Distinct evidence |
| --- | --- | --- | --- |
| Cube | 5 / 0 | 100 HP | Bots see the target while wandering; partial routes end roughly 8.54 units below/short. Physical stacking remains absent. |
| Mattress house | 17,518 / 0 | 100 HP | Partial chase/search routes stop roughly 8 units short, then visible targets coexist with wandering. |
| City tower | 8,528 / 0 | 100 HP | Best partial routes reach about Y=289.8 versus target Y=315.6, then pursuit stops despite sight. |
| Long staircase | 82 / 0 | 100 HP | Routes end near Y=286.4 under the high target instead of discovering the stair ascent; wandering persists with sight. |
| House exit, supplied position | 158 / 0 | Killed | A hit eventually lands after about 14 seconds. This alone does not prove prompt traversal through the exit. |

A separate house-egress variant moves the target eight units farther outward
along +X, beyond immediate window melee reach. It preserves the same save and
reports the actual ground-adjusted feet. The bot observes the target briefly,
retains pursuit evidence, fails to leave promptly and respawns at tick 2041
(previous life began at tick 30). This reproduces the reported stuck/suicide
boundary more clearly than treating the nearby target's death as egress success.

Private JSONs/logs and `v043-bot-baseline-summary.json` are under the preserved
evidence `diagnostics/` directory. The cube's earlier placement run is retained
separately. Baseline Bedroom clock schedules remain explicitly unsupported in
the host; that unrelated limit is reported rather than blocking route observation.
No original-reference fidelity or Maxwell acceptance is claimed by these probes.

Next supported corrections: trace native stair/door support and clearance through
the existing navigation, then separate a failed route from target hostility and
pursuit. Current `out_of_reach` filtering removes a still-visible enemy from
behavior scoring within 2.5 units of the failed goal. Preserve bounded failed-route
reuse without turning that into wandering away from a known elevated target.
Retest actual ascent and egress independently of the later physical stacking work.

Also reproduced issue 414 with the supplied original Bot_Town ZIP: the shipping
importer emits eight missing-source brick entries but no usable spawn geometry;
the shipping native checker still returns OK. Private output is preserved at
`E:/Downloads/BlocklandReImagined-Release-Evidence/v043-bot-town-before`.
Conversion dependency resolution and native catalogue validation both remain open.
