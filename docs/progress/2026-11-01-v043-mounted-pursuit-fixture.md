# Mounted-pursuit fixture positioning

The mounted-pursuit test's diagonal variant autonomously boarded and completed
the chase, but its second layout required the bot to spend about 40 units
walking to a harmless, unarmed vehicle even when pursuing the target on foot
was a plausible choice. That prerequisite prevented the second layout from
exercising the driver behavior the test names.

For the flank case only, moved the bot's authored spawn closer to the vehicle
and measured its walking leash from the new actual home. Both variants must
still board through the normal bot decision/control path, remain in the driver
seat, pursue a genuinely retreating target beyond the walking leash, close the
gap and satisfy the existing reversing/steering assertions. The diagonal
variant retains the 40-unit approach. In a focused run the flank gap decreased
from 22.25 to 10.89 units and the diagonal from 19.80 to 11.60.

All 24 `bot_interactions` tests, formatting and changed-target Clippy passed in
the Windows playtest profile. This does not prove the original farther-away
flank choice was optimal or that all vehicle opportunities are solved; #423
retains that context pending the full gate.
