# Balcony pursuit bounded inspection

Inspected the existing mattress-house candidate-3 saved-world trace, rather
than inventing a new outcome from the first-kill time. At tick 607, bots 2 and
4 had leap legs toward points around Y=313 while their feet were at Y=307 and
Y=310. By tick 1087 they were back near Y=305 and Y=308 with fresh searches;
the sampled trace is consistent with failed difficult approaches, although it
does not prove the precise collision or control cause. By tick 1567 all four
had ordinary walking routes away from the target toward the house entrance;
bot 3 was on the upper route at tick 2527 and reached the target at tick 3007.
This confirms both the delay and eventual use of the longer route in that run.

The existing planner charges a leap's measured stop and travel time, plus a
penalty for an overhanging landing, and temporarily avoids a specific missed
leap after an observed failure. The samples do not establish whether a general
leap penalty, a particular clearance issue, crowd interference, or an earlier
partial route explains this result. The previous safety-cost candidate failed
the saved scenario, so an unmeasured global cost change would risk a regression.
Under Maxwell's bounded-investigation instruction, retain this as an explicit
limitation rather than claim the balcony is reliably solved or tune blindly.
The trace is private `v043-balcony-feedback-candidate3.json`; it predates the
latest bot spawn lifecycle adjustment and is not a final-build replay.
